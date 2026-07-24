//! Unified workflow execution engine for `beskid hi`.
//! Powered by workflow-rs (workflow-task + workflow-core).
//!
//! Replaces the old dual-path model (in-process HiCompileJob + subprocess CliRunPlan)
//! with typed async workflow stages that run entirely in-process.
//!
//! Each stage (Build, Test, Run, Analyze, Graph) is a `workflow_task::Task<A, T>`
//! with lifecycle management (run, stop, join, stop_and_join) and a stop-signal channel.
//! Progress is reported via `Sender<WorkflowEvent>` back to the shell for UI updates.

use std::sync::mpsc;

use async_channel::Receiver as StopReceiver;
use workflow_task::{Task, TaskResult};

use super::hi_compile::{HiCompileRegistrar, HiCompileRequest};
use super::scope::ShellScope;

/// Pipeline stages the workflow engine can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkflowStage {
    Build,
    Test,
    Run,
    Analyze,
    Graph,
}

impl WorkflowStage {
    pub fn label(self) -> &'static str {
        match self {
            WorkflowStage::Build => "build",
            WorkflowStage::Test => "test",
            WorkflowStage::Run => "run",
            WorkflowStage::Analyze => "analyze",
            WorkflowStage::Graph => "graph",
        }
    }
}

/// Commands submitted to the workflow engine.
#[derive(Debug, Clone)]
pub enum WorkflowCommand {
    Build { params: String },
    Test { params: String },
    Run { target: String, args: Vec<String> },
    Analyze { params: String },
    Graph { params: String },
    Cancel,
}

/// Events emitted from the engine to the shell for UI updates.
#[derive(Debug, Clone)]
pub enum WorkflowEvent {
    StageStarted(WorkflowStage),
    Progress(WorkflowStage, f32, String),
    Log(WorkflowStage, String),
    StageCompleted(WorkflowStage),
    StageFailed(WorkflowStage, String),
    AllComplete,
    Cancelled,
}

/// Output produced by a single stage execution.
#[derive(Debug)]
pub struct StageOutput {
    pub stage: WorkflowStage,
    pub success: bool,
    pub message: String,
}

/// A typed stage runner backed by `workflow_task::Task`.
///
/// Each stage runs as an async Task with:
/// - Typed input parameters (String)
/// - A stop signal channel (`Receiver<()>`) for cancellation
/// - A progress channel (`Sender<WorkflowEvent>`) for UI updates
/// - A return value (`StageOutput`) on completion
pub struct StageRunner {
    task: Task<StageInput, StageOutput>,
}

pub struct StageInput {
    params: String,
    scope: ShellScope,
    tx: mpsc::Sender<WorkflowEvent>,
}

impl StageRunner {
    /// Create a new stage runner with the given async worker function.
    /// The worker receives (StageInput, StopReceiver<()>) and returns StageOutput.
    pub fn new<F>(stage: WorkflowStage, worker: F) -> Self
    where
        F: Fn(StageInput, StopReceiver<()>) -> StageOutput + Clone + Send + Sync + 'static,
    {
        let task = Task::new(move |input: StageInput, stop: StopReceiver<()>| {
            let worker = worker.clone();
            Box::pin(async move {
                if stop.try_recv().is_ok() {
                    let _ = input.tx.send(WorkflowEvent::Cancelled);
                    return StageOutput { stage, success: false, message: "Cancelled".into() };
                }
                let _ = input.tx.send(WorkflowEvent::StageStarted(stage));
                worker(input, stop)
            })
        });
        Self { task }
    }

    /// Run the stage with the given input.
    pub fn run(&self, input: StageInput) -> TaskResult<&Self> {
        self.task.run(input).map(|_| self)
    }

    /// Signal the stage to stop (cancellation).
    pub fn stop(&self) -> TaskResult<()> {
        self.task.stop()
    }

    /// Check if the stage is currently running.
    pub fn is_running(&self) -> bool {
        self.task.is_running()
    }
}

// ---------------------------------------------------------------------------
// WorkflowEngine
// ---------------------------------------------------------------------------

/// Central workflow coordinator that owns stage runners and manages execution.
///
/// Usage (inside a shell app that owns both):
/// ```ignore
/// engine.submit(WorkflowCommand::Build { params: "".into() }, scope.clone());
/// // In the event loop tick:
/// for event in engine.drain_events() { /* update UI */ }
/// ```
pub struct WorkflowEngine {
    build_runner: StageRunner,
    test_runner: StageRunner,
    run_runner: StageRunner,
    analyze_runner: StageRunner,
    graph_runner: StageRunner,

    event_tx: mpsc::Sender<WorkflowEvent>,
    event_rx: mpsc::Receiver<WorkflowEvent>,
    current_stage: Option<WorkflowStage>,
}

impl WorkflowEngine {
    pub fn new(compile_registrar: Option<HiCompileRegistrar>) -> Self {
        let (event_tx, event_rx) = mpsc::channel();

        // Build the stage runners with the compile_registrar wired in
        let registrar = compile_registrar;
        let build_runner = StageRunner::new(WorkflowStage::Build, {
            move |input: StageInput, stop: StopReceiver<()>| {
                if stop.try_recv().is_ok() {
                    return StageOutput { stage: WorkflowStage::Build, success: false, message: "Cancelled".into() };
                }
                if let Some(reg) = registrar {
                    let _ = input.tx.send(WorkflowEvent::Log(WorkflowStage::Build, "Compiling...".into()));
                    let result = reg(HiCompileRequest {
                        command: "build",
                        params: &input.params,
                        scope: &input.scope,
                        msg_tx: mpsc::channel().0, // TODO: bridge RuntimeOp -> WorkflowEvent
                    });
                    match result {
                        Ok(()) => StageOutput {
                            stage: WorkflowStage::Build,
                            success: true,
                            message: "Build succeeded".into(),
                        },
                        Err(e) => StageOutput {
                            stage: WorkflowStage::Build,
                            success: false,
                            message: format!("Build failed: {e}"),
                        },
                    }
                } else {
                    StageOutput {
                        stage: WorkflowStage::Build,
                        success: false,
                        message: "No compile registrar available".into(),
                    }
                }
            }
        });

        // For now, test runner just delegates to the compile_registrar too
        let test_registrar = compile_registrar;
        let test_runner = StageRunner::new(WorkflowStage::Test, {
            move |input: StageInput, stop: StopReceiver<()>| {
                if stop.try_recv().is_ok() {
                    return StageOutput { stage: WorkflowStage::Test, success: false, message: "Cancelled".into() };
                }
                if let Some(reg) = test_registrar {
                    let result = reg(HiCompileRequest {
                        command: "test",
                        params: &input.params,
                        scope: &input.scope,
                        msg_tx: mpsc::channel().0,
                    });
                    match result {
                        Ok(()) => {
                            StageOutput { stage: WorkflowStage::Test, success: true, message: "Tests passed".into() }
                        }
                        Err(e) => StageOutput {
                            stage: WorkflowStage::Test,
                            success: false,
                            message: format!("Tests failed: {e}"),
                        },
                    }
                } else {
                    StageOutput {
                        stage: WorkflowStage::Test,
                        success: false,
                        message: "No compile registrar available".into(),
                    }
                }
            }
        });

        let run_registrar = compile_registrar;
        let run_runner = StageRunner::new(WorkflowStage::Run, {
            move |input: StageInput, stop: StopReceiver<()>| {
                if stop.try_recv().is_ok() {
                    return StageOutput { stage: WorkflowStage::Run, success: false, message: "Cancelled".into() };
                }
                if let Some(reg) = run_registrar {
                    let result = reg(HiCompileRequest {
                        command: "run",
                        params: &input.params,
                        scope: &input.scope,
                        msg_tx: mpsc::channel().0,
                    });
                    match result {
                        Ok(()) => {
                            StageOutput { stage: WorkflowStage::Run, success: true, message: "Run succeeded".into() }
                        }
                        Err(e) => StageOutput {
                            stage: WorkflowStage::Run,
                            success: false,
                            message: format!("Run failed: {e}"),
                        },
                    }
                } else {
                    StageOutput {
                        stage: WorkflowStage::Run,
                        success: false,
                        message: "No compile registrar available".into(),
                    }
                }
            }
        });

        let analyze_registrar = compile_registrar;
        let analyze_runner = StageRunner::new(WorkflowStage::Analyze, {
            move |input: StageInput, stop: StopReceiver<()>| {
                if stop.try_recv().is_ok() {
                    return StageOutput { stage: WorkflowStage::Analyze, success: false, message: "Cancelled".into() };
                }
                if let Some(reg) = analyze_registrar {
                    let result = reg(HiCompileRequest {
                        command: "analyze",
                        params: &input.params,
                        scope: &input.scope,
                        msg_tx: mpsc::channel().0,
                    });
                    match result {
                        Ok(()) => StageOutput {
                            stage: WorkflowStage::Analyze,
                            success: true,
                            message: "Analyze succeeded".into(),
                        },
                        Err(e) => StageOutput {
                            stage: WorkflowStage::Analyze,
                            success: false,
                            message: format!("Analyze failed: {e}"),
                        },
                    }
                } else {
                    StageOutput {
                        stage: WorkflowStage::Analyze,
                        success: false,
                        message: "No compile registrar available".into(),
                    }
                }
            }
        });

        let graph_registrar = compile_registrar;
        let graph_runner = StageRunner::new(WorkflowStage::Graph, {
            move |input: StageInput, stop: StopReceiver<()>| {
                if stop.try_recv().is_ok() {
                    return StageOutput { stage: WorkflowStage::Graph, success: false, message: "Cancelled".into() };
                }
                if let Some(reg) = graph_registrar {
                    let result = reg(HiCompileRequest {
                        command: "graph",
                        params: &input.params,
                        scope: &input.scope,
                        msg_tx: mpsc::channel().0,
                    });
                    match result {
                        Ok(()) => StageOutput {
                            stage: WorkflowStage::Graph,
                            success: true,
                            message: "Graph succeeded".into(),
                        },
                        Err(e) => StageOutput {
                            stage: WorkflowStage::Graph,
                            success: false,
                            message: format!("Graph failed: {e}"),
                        },
                    }
                } else {
                    StageOutput {
                        stage: WorkflowStage::Graph,
                        success: false,
                        message: "No compile registrar available".into(),
                    }
                }
            }
        });

        Self {
            build_runner,
            test_runner,
            run_runner,
            analyze_runner,
            graph_runner,

            event_tx,
            event_rx,
            current_stage: None,
        }
    }

    /// Submit a command to the workflow engine.
    pub fn submit(&mut self, command: WorkflowCommand, scope: ShellScope) {
        if self.is_running() {
            return; // Cannot run parallel stages
        }
        let tx = self.event_tx.clone();
        match command {
            WorkflowCommand::Build { params } => {
                let _ = self.build_runner.run(StageInput { params, scope, tx });
                self.current_stage = Some(WorkflowStage::Build);
            }
            WorkflowCommand::Test { params } => {
                let _ = self.test_runner.run(StageInput { params, scope, tx });
                self.current_stage = Some(WorkflowStage::Test);
            }
            WorkflowCommand::Run { target, args } => {
                let params = if args.is_empty() { target } else { format!("{} {}", target, args.join(" ")) };
                let _ = self.run_runner.run(StageInput { params, scope, tx });
                self.current_stage = Some(WorkflowStage::Run);
            }
            WorkflowCommand::Analyze { params } => {
                let _ = self.analyze_runner.run(StageInput { params, scope, tx });
                self.current_stage = Some(WorkflowStage::Analyze);
            }
            WorkflowCommand::Graph { params } => {
                let _ = self.graph_runner.run(StageInput { params, scope, tx });
                self.current_stage = Some(WorkflowStage::Graph);
            }
            WorkflowCommand::Cancel => {
                self.cancel();
            }
        }
    }

    /// Cancel the current running stage.
    pub fn cancel(&mut self) {
        if let Some(stage) = self.current_stage {
            match stage {
                WorkflowStage::Build => {
                    let _ = self.build_runner.stop();
                }
                WorkflowStage::Test => {
                    let _ = self.test_runner.stop();
                }
                WorkflowStage::Run => {
                    let _ = self.run_runner.stop();
                }
                WorkflowStage::Analyze => {
                    let _ = self.analyze_runner.stop();
                }
                WorkflowStage::Graph => {
                    let _ = self.graph_runner.stop();
                }
            }
            self.current_stage = None;
        }
    }

    /// Check if a stage is currently running.
    pub fn is_running(&self) -> bool {
        self.current_stage.is_some()
    }

    /// Drain all pending events from the engine.
    /// Returns events and updates internal state.
    pub fn drain_events(&mut self) -> Vec<WorkflowEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            match &event {
                WorkflowEvent::StageCompleted(stage) | WorkflowEvent::StageFailed(stage, _) => {
                    if self.current_stage == Some(*stage) {
                        self.current_stage = None;
                    }
                }
                WorkflowEvent::Cancelled => {
                    self.current_stage = None;
                }
                _ => {}
            }
            events.push(event);
        }
        events
    }

    pub fn current_stage(&self) -> Option<WorkflowStage> {
        self.current_stage
    }
}

impl Default for WorkflowEngine {
    fn default() -> Self {
        Self::new(None)
    }
}

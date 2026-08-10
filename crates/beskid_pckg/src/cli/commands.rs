use super::output::{print_package_versions_table, print_pckg_error_human};
use super::pack::execute_pack;
use super::repository::{build_client, execute_configure};
use super::{
    DetailsArgs, DownloadArgs, Instant, IsTerminal, PckgArgs, PckgClient, PckgCommand, PckgError, PublishArgs,
    SearchArgs, UploadProgress, VersionActionArgs, VersionsArgs, fs, io,
};
use tracing::error;

/// Run `args.command` on a fresh multi-thread Tokio runtime (`block_on` internally).
pub fn execute(args: PckgArgs) -> Result<(), PckgError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| PckgError::RuntimeInit(err.to_string()))?;

    runtime.block_on(execute_async(args))
}

async fn execute_async(args: PckgArgs) -> Result<(), PckgError> {
    let args_for_client = args.clone();
    let command = command_name(&args.command);
    let started = Instant::now();
    if args.verbose {
        let auth =
            if args.api_key.is_some() || args.bearer_token.is_some() { "cli-args" } else { "repositories.json-or-env" };
        eprintln!("[pckg] verbose: base_url={} auth_hint={auth}", args.base_url.trim());
    }

    tracing::info!(
        target: "beskid.pckg",
        command = command,
        base_url = args.base_url.as_str(),
        "pckg command started"
    );

    let result = match args.command {
        PckgCommand::Pack(pack_args) => execute_pack(pack_args),
        PckgCommand::Upload(upload_args) => {
            let client = build_client(&args_for_client)?;
            execute_publish(&client, upload_args, args_for_client.verbose).await
        }
        PckgCommand::Configure(configure_args) => execute_configure(&args.config_file, &args.base_url, configure_args),
        PckgCommand::List => {
            let client = build_client(&args_for_client)?;
            execute_list(&client).await
        }
        PckgCommand::Search(search_args) => {
            let client = build_client(&args_for_client)?;
            execute_search(&client, search_args).await
        }
        PckgCommand::Details(details_args) => {
            let client = build_client(&args_for_client)?;
            execute_details(&client, details_args).await
        }
        PckgCommand::Versions(versions_args) => {
            let client = build_client(&args_for_client)?;
            execute_versions(&client, versions_args).await
        }
        PckgCommand::Download(download_args) => {
            let client = build_client(&args_for_client)?;
            execute_download(&client, download_args).await
        }
        PckgCommand::Yank(action_args) => {
            let client = build_client(&args_for_client)?;
            execute_yank(&client, action_args).await
        }
        PckgCommand::Unyank(action_args) => {
            let client = build_client(&args_for_client)?;
            execute_unyank(&client, action_args).await
        }
        PckgCommand::Whoami => {
            let client = build_client(&args_for_client)?;
            execute_whoami(&client).await
        }
    };

    match &result {
        Ok(()) => {
            tracing::info!(
                target: "beskid.pckg",
                command = command,
                elapsed_ms = started.elapsed().as_millis(),
                "pckg command completed"
            );
        }
        Err(error) => {
            emit_command_error(command, error);
        }
    }

    result
}

fn command_name(command: &PckgCommand) -> &'static str {
    match command {
        PckgCommand::Pack(_) => "pack",
        PckgCommand::Upload(_) => "upload",
        PckgCommand::Configure(_) => "configure",
        PckgCommand::List => "list",
        PckgCommand::Search(_) => "search",
        PckgCommand::Details(_) => "details",
        PckgCommand::Versions(_) => "versions",
        PckgCommand::Download(_) => "download",
        PckgCommand::Yank(_) => "yank",
        PckgCommand::Unyank(_) => "unyank",
        PckgCommand::Whoami => "whoami",
    }
}

fn emit_command_error(command: &'static str, error: &PckgError) {
    let category = match error {
        PckgError::MissingAuthToken => "missing_auth_token",
        PckgError::Url(_) => "invalid_url",
        PckgError::Transport(_) => "network_transport",
        PckgError::Io(_) => "io_error",
        PckgError::RuntimeInit(_) => "runtime_init",
        PckgError::Api { .. } => "api_error",
        PckgError::LogicalFailure { .. } => "logical_failure",
    };

    if matches!(error, PckgError::MissingAuthToken) {
        tracing::warn!(
            target: "beskid.pckg",
            command = command,
            error_category = category,
            "pckg command failed"
        );
        return;
    }

    error!(
        target: "beskid.pckg",
        command = command,
        error_category = category,
        "pckg command failed"
    );
}
async fn execute_publish(client: &PckgClient, args: PublishArgs, verbose: bool) -> Result<(), PckgError> {
    let artifact_name = args.artifact.file_name().and_then(|name| name.to_str()).unwrap_or("artifact.bpk").to_string();

    let artifact_path = &args.artifact;
    let len = tokio::fs::metadata(artifact_path).await.map_err(PckgError::Io)?.len();

    let upload_progress = if io::stderr().is_terminal() && len > 0 { Some(UploadProgress::new(len)) } else { None };

    if upload_progress.is_none() {
        eprintln!("Publishing package version...");
    }

    let started = Instant::now();
    let response = client
        .publish_package_version(
            &args.package,
            None,
            artifact_path,
            &artifact_name,
            args.manifest_json.as_deref(),
            args.checksum_sha256.as_deref(),
            upload_progress.as_ref(),
        )
        .await;

    match response {
        Ok(response) => {
            if verbose {
                eprintln!("[pckg] verbose: upload elapsed {:?}", started.elapsed());
            }
            let base = client.config().base_url.as_str().trim_end_matches('/');
            println!("{}", response.message);
            println!("--- publish summary ---");
            println!("registry: {base}");
            println!("request:  POST /api/packages/{}/publish", args.package.trim());
            println!("package:  {}", args.package);
            if let Some(version) = &response.version {
                println!("PCKG_PUBLISHED_VERSION={}", version.version);
                println!("version:  {} (registry-assigned)", version.version);
                println!("checksum: {}", version.checksum_sha256);
                println!("size:     {} bytes", version.size_bytes);
                println!("published_at_utc: {}", version.published_at_utc);
                print_package_versions_table(std::slice::from_ref(version));
            } else {
                println!("version:  (not returned by registry — check `beskid pckg versions {}`)", args.package.trim());
            }
            println!("------------------------");
            Ok(())
        }
        Err(err) => {
            print_pckg_error_human(&err);
            Err(err)
        }
    }
}

async fn execute_list(client: &PckgClient) -> Result<(), PckgError> {
    let items = client.list_packages().await?;
    if items.is_empty() {
        println!("No packages found.");
        return Ok(());
    }

    for item in items {
        println!(
            "{} [{}] downloads={} rating={:.2}",
            item.name, item.category, item.total_downloads, item.average_rating
        );
    }
    Ok(())
}

async fn execute_search(client: &PckgClient, args: SearchArgs) -> Result<(), PckgError> {
    let items = client.search_packages(&args.query).await?;
    if items.is_empty() {
        println!("No packages matched '{}'.", args.query);
        return Ok(());
    }

    for item in items {
        println!(
            "{} [{}/{}] score={:.2} reviews={}",
            item.package.name, item.health.state, item.health.sub_state, item.health.score, item.review_count
        );
    }
    Ok(())
}

async fn execute_details(client: &PckgClient, args: DetailsArgs) -> Result<(), PckgError> {
    let details = client.get_package_details(&args.id_or_name).await?;
    println!(
        "{} ({}) downloads={} dependents={}",
        details.package.name, details.package.category, details.package.total_downloads, details.dependents_count
    );
    println!("health={}/{} score={:.2}", details.health.state, details.health.sub_state, details.health.score);
    if !details.dependencies.is_empty() {
        println!("dependencies:");
        for dep in details.dependencies {
            println!(
                "- {} {} source={} registry={}",
                dep.name,
                dep.version.unwrap_or_else(|| "*".to_string()),
                dep.source,
                dep.registry.unwrap_or_else(|| "-".to_string())
            );
        }
    }
    Ok(())
}

async fn execute_versions(client: &PckgClient, args: VersionsArgs) -> Result<(), PckgError> {
    let versions = client.list_package_versions(&args.package).await?;
    if versions.is_empty() {
        println!("No versions found for {}.", args.package);
        return Ok(());
    }
    print_package_versions_table(&versions);
    Ok(())
}

async fn execute_download(client: &PckgClient, args: DownloadArgs) -> Result<(), PckgError> {
    let bytes = client.download_package_version(&args.package, &args.version).await?;
    if let Some(parent) = args.output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.output, bytes)?;
    println!("Downloaded {} {} to {}", args.package, args.version, args.output.display());
    Ok(())
}

async fn execute_yank(client: &PckgClient, args: VersionActionArgs) -> Result<(), PckgError> {
    let response = client.yank_package_version(&args.package, &args.version).await?;
    println!("{}", response.message);
    Ok(())
}

async fn execute_unyank(client: &PckgClient, args: VersionActionArgs) -> Result<(), PckgError> {
    let response = client.unyank_package_version(&args.package, &args.version).await?;
    println!("{}", response.message);
    Ok(())
}

async fn execute_whoami(client: &PckgClient) -> Result<(), PckgError> {
    let me = client.current_user().await?;
    println!(
        "authenticated={} user_id={} email={} publisher={}",
        me.is_authenticated,
        me.user_id.unwrap_or_else(|| "-".to_string()),
        me.email.unwrap_or_else(|| "-".to_string()),
        me.is_publisher
    );
    Ok(())
}

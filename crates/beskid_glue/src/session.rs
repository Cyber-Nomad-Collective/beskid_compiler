//! Single-owner protocol state. An existing cooperative process/pipe pump drives
//! this state; it starts no threads, scheduler, runtime, or native owner registry.
use crate::{codec::{BinaryDecoder,BinaryEncoder,glue_limits},wire::{Envelope,MessageKind,WireError}};
use beskid_serialization::{DataValue,Error as ValueError};
use std::time::{Duration,Instant};
#[derive(Debug,PartialEq,Eq)]
pub enum SessionError { Wire(WireError),Value(ValueError),Handshake,Closed,Outstanding,Exhausted,UnknownRequest,LateReply,Binding }
impl From<WireError> for SessionError {fn from(error:WireError)->Self{Self::Wire(error)}}
impl From<ValueError> for SessionError {fn from(error:ValueError)->Self{Self::Value(error)}}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum Phase { Handshake,Ready,Closing,Closed,Failed }
struct Pending { id:u64,binding:[u8;32],shapes:Vec<u8> }
/// Teardown must not allocate while settling waiters after an allocation error.
#[derive(Debug,PartialEq,Eq)]
pub struct SettledRequests { ids:[u64;256],length:usize }
impl SettledRequests {pub fn as_slice(&self)->&[u64]{&self.ids[..self.length]}}
pub struct Session {
    library:String,generation:u64,handshake:DataValue,phase:Phase,next:u64,
    pending:Vec<Pending>,close_deadline:Option<Instant>,shutdown_acknowledged:bool,
}
impl Session {
    pub fn new(library:String,generation:u64,handshake:DataValue)->Result<Self,SessionError> {
        // Validate descriptive handshake data and complete envelope before any
        // process launch. It does not authorize callable/image registration.
        let payload=beskid_serialization::encode(&handshake,&mut BinaryEncoder::default(),&glue_limits())?;
        Envelope{kind:MessageKind::Handshake,generation,request:0,library:&library,binding:&[0;32],shapes:&[],payload:&payload}.encode_frame()?;
        let mut pending=Vec::new();pending.try_reserve_exact(256).map_err(|_|ValueError::AllocationFailure)?;
        Ok(Self{library,generation,handshake,phase:Phase::Handshake,next:1,pending,close_deadline:None,shutdown_acknowledged:false})
    }
    pub fn phase(&self)->Phase {self.phase}
    pub fn outstanding(&self)->usize {self.pending.len()}
    fn frame(&self,kind:MessageKind,id:u64,binding:&[u8;32],shapes:&[u8],payload:&[u8])->Result<Vec<u8>,SessionError> {
        Ok(Envelope{kind,generation:self.generation,request:id,library:&self.library,binding,shapes,payload}.encode_frame()?)
    }
    pub fn handshake_frame(&self)->Result<Vec<u8>,SessionError> {
        if self.phase!=Phase::Handshake {return Err(SessionError::Handshake)}
        let payload=beskid_serialization::encode(&self.handshake,&mut BinaryEncoder::default(),&glue_limits())?;
        self.frame(MessageKind::Handshake,0,&[0;32],&[],&payload)
    }
    /// A request consumes its ID before publication. No failure can reuse it.
    pub fn request(&mut self,binding:[u8;32],shapes:&[u8],value:&DataValue)->Result<(u64,Vec<u8>),SessionError> {
        if self.phase!=Phase::Ready {return Err(SessionError::Closed)}
        if self.pending.len()==256 {return Err(SessionError::Outstanding)}
        let id=self.next;
        self.next=match id.checked_add(1) {Some(next)=>next,None=>{self.phase=Phase::Failed;return Err(SessionError::Exhausted)}};
        let payload=beskid_serialization::encode(value,&mut BinaryEncoder::default(),&glue_limits())?;
        let frame=self.frame(MessageKind::Request,id,&binding,shapes,&payload)?;
        let mut retained=Vec::new();retained.try_reserve_exact(shapes.len()).map_err(|_|ValueError::AllocationFailure)?;
        retained.extend_from_slice(shapes);
        self.pending.push(Pending{id,binding,shapes:retained});Ok((id,frame))
    }
    /// Removes the waiter before publishing cancellation. A late response is
    /// classified by the issued ID range and cannot recreate pending state.
    pub fn cancel(&mut self,id:u64)->Result<Vec<u8>,SessionError> {
        if self.phase!=Phase::Ready {return Err(SessionError::Closed)}
        let index=self.pending.iter().position(|pending|pending.id==id).ok_or(SessionError::UnknownRequest)?;
        let pending=self.pending.remove(index);
        self.frame(MessageKind::Cancel,id,&pending.binding,&pending.shapes,&[])
    }
    /// Complete reply values remain owned. The caller settles its corresponding
    /// fiber exactly once; no callback runs inside this parser/state authority.
    pub fn receive(&mut self,frame:&[u8])->Result<Option<(u64,bool,DataValue)>,SessionError> {
        let result=self.receive_inner(frame);
        if result.is_err() && !matches!(result,Err(SessionError::LateReply)) {
            // Preserve the waiter IDs until the pump drains them through close
            // or peer_exited; a protocol error must not silently strand fibers.
            self.phase=Phase::Failed;
        }
        result
    }
    fn receive_inner(&mut self,frame:&[u8])->Result<Option<(u64,bool,DataValue)>,SessionError> {
        let envelope=Envelope::decode_frame(frame)?;
        if envelope.library!=self.library || envelope.generation!=self.generation {return Err(SessionError::Handshake)}
        if self.phase==Phase::Handshake {
            if envelope.kind!=MessageKind::Acknowledgement || envelope.request!=0 || *envelope.binding!=[0;32] || !envelope.shapes.is_empty() {return Err(SessionError::Handshake)}
            let value=beskid_serialization::decode(envelope.payload,&mut BinaryDecoder,&glue_limits())?;
            if value!=self.handshake {return Err(SessionError::Handshake)}
            self.phase=Phase::Ready;return Ok(None)
        }
        if self.phase==Phase::Closing {
            if envelope.kind==MessageKind::Acknowledgement && envelope.request==0 && *envelope.binding==[0;32] && envelope.shapes.is_empty() && envelope.payload.is_empty() {
                // A protocol acknowledgement is not proof that the OS child
                // exited or was reaped. Keep the process deadline active.
                self.shutdown_acknowledged=true;return Ok(None)
            }
            return Err(SessionError::Closed)
        }
        if self.phase!=Phase::Ready {return Err(SessionError::Closed)}
        if !matches!(envelope.kind,MessageKind::Response|MessageKind::Error) {return Err(SessionError::Binding)}
        if envelope.request==0 {return Err(SessionError::UnknownRequest)}
        let Some(index)=self.pending.iter().position(|pending|pending.id==envelope.request) else {
            return Err(if envelope.request<self.next {SessionError::LateReply}else{SessionError::UnknownRequest})
        };
        let pending=&self.pending[index];
        if *envelope.binding!=pending.binding || envelope.shapes!=pending.shapes {return Err(SessionError::Binding)}
        let value=beskid_serialization::decode(envelope.payload,&mut BinaryDecoder,&glue_limits())?;
        let id=self.pending.remove(index).id;
        Ok(Some((id,envelope.kind==MessageKind::Error,value)))
    }
    /// Callers settle all drained IDs and release their resources before waiting
    /// for the peer. The process adapter owns termination/reap at this deadline.
    pub fn close(&mut self,now:Instant)->Result<(SettledRequests,Option<Vec<u8>>),SessionError> {
        if matches!(self.phase,Phase::Closing|Phase::Closed) {return Ok((SettledRequests{ids:[0;256],length:0},None))}
        let deadline=now.checked_add(Duration::from_secs(5)).ok_or(SessionError::Exhausted)?;
        if self.phase==Phase::Failed {
            let ids=self.drain();self.phase=Phase::Closing;self.close_deadline=Some(deadline);
            return Ok((ids,None))
        }
        let frame=self.frame(MessageKind::Shutdown,0,&[0;32],&[],&[])?;
        let ids=self.drain();
        self.phase=Phase::Closing;self.close_deadline=Some(deadline);
        Ok((ids,Some(frame)))
    }
    pub fn close_deadline(&self)->Option<Instant> {self.close_deadline}
    pub fn shutdown_acknowledged(&self)->bool {self.shutdown_acknowledged}
    /// EOF/process exit is terminal and the pump settles these IDs as failures.
    fn drain(&mut self)->SettledRequests {
        let mut result=SettledRequests{ids:[0;256],length:self.pending.len()};
        for (index,pending) in self.pending.iter().enumerate(){result.ids[index]=pending.id;}
        self.pending.clear();result
    }
    pub fn peer_exited(&mut self)->SettledRequests {
        let ids=self.drain();self.phase=Phase::Closed;ids
    }
}
#[cfg(test)] mod tests {
    use super::*;
    fn ready()->Session {
        let mut session=Session::new("peer".into(),7,DataValue::String("target/abi/layout/bindings".into())).unwrap();
        let payload=beskid_serialization::encode(&session.handshake,&mut BinaryEncoder::default(),&glue_limits()).unwrap();
        let ack=session.frame(MessageKind::Acknowledgement,0,&[0;32],&[],&payload).unwrap();
        assert_eq!(session.receive(&ack).unwrap(),None);session
    }
    #[test] fn cancel_and_duplicate_reply_cannot_resurrect_waiters() {
        let mut session=ready();let (id,_)=session.request([1;32],&[2;32],&DataValue::Unit).unwrap();
        session.cancel(id).unwrap();assert_eq!(session.outstanding(),0);
        let reply=session.frame(MessageKind::Response,id,&[1;32],&[2;32],&[0]).unwrap();
        assert_eq!(session.receive(&reply),Err(SessionError::LateReply));assert_eq!(session.phase(),Phase::Ready);
        let (next,_)=session.request([1;32],&[],&DataValue::Unit).unwrap();assert!(next>id);
        let reply=session.frame(MessageKind::Response,next,&[1;32],&[],&[0]).unwrap();
        assert_eq!(session.receive(&reply).unwrap(),Some((next,false,DataValue::Unit)));
        assert_eq!(session.receive(&reply),Err(SessionError::LateReply));
    }
    #[test] fn foreign_binding_is_terminal_and_close_is_bounded_idempotent() {
        let mut session=ready();let (id,_)=session.request([1;32],&[],&DataValue::Unit).unwrap();
        let reply=session.frame(MessageKind::Response,id,&[3;32],&[],&[0]).unwrap();
        assert_eq!(session.receive(&reply),Err(SessionError::Binding));
        assert_eq!(session.peer_exited().as_slice(),&[id]);assert_eq!(session.outstanding(),0);
        let mut session=ready();let (id,_)=session.request([1;32],&[],&DataValue::Unit).unwrap();
        let now=Instant::now();let (cancelled,frame)=session.close(now).unwrap();assert_eq!(cancelled.as_slice(),&[id]);assert!(frame.is_some());
        assert_eq!(session.close_deadline().unwrap().duration_since(now),Duration::from_secs(5));
        let (cancelled,frame)=session.close(now).unwrap();assert!(cancelled.as_slice().is_empty());assert!(frame.is_none());assert_eq!(session.outstanding(),0);
    }
    #[test] fn outstanding_cap_and_handshake_mismatch_prevent_dispatch() {
        let mut session=ready();for _ in 0..256 {session.request([1;32],&[],&DataValue::Unit).unwrap();}
        assert_eq!(session.request([1;32],&[],&DataValue::Unit),Err(SessionError::Outstanding));
        let mut session=Session::new("peer".into(),7,DataValue::Unit).unwrap();
        let ack=session.frame(MessageKind::Acknowledgement,0,&[0;32],&[],&[1,1]).unwrap();
        assert_eq!(session.receive(&ack),Err(SessionError::Handshake));assert_eq!(session.phase(),Phase::Failed);
    }
    #[test] fn corrupted_control_fields_and_unissued_zero_cannot_settle_calls() {
        let session=ready();
        let mut handshake=Session::new("peer".into(),7,DataValue::Unit).unwrap();
        let ack=handshake.frame(MessageKind::Acknowledgement,0,&[0;32],&[],&[0]).unwrap();
        let binding_offset=28+"peer".len();
        let mut wrong=ack.clone();wrong[binding_offset]=1;
        assert!(handshake.receive(&wrong).is_err());
        let mut closing=ready();closing.close(Instant::now()).unwrap();
        assert!(closing.receive(&wrong).is_err());
        let mut ready=ready();let mut response=session.frame(MessageKind::Response,1,&[1;32],&[],&[0]).unwrap();
        response[16..24].copy_from_slice(&0u64.to_le_bytes());
        assert!(ready.receive(&response).is_err());assert_eq!(ready.phase(),Phase::Failed);
    }
    #[test] fn failed_protocol_and_shutdown_ack_keep_process_reap_deadline() {
        let mut session=ready();let (id,_)=session.request([1;32],&[],&DataValue::Unit).unwrap();
        assert!(session.receive(&[0]).is_err());let now=Instant::now();
        let (ids,frame)=session.close(now).unwrap();assert_eq!(ids.as_slice(),&[id]);assert!(frame.is_none());
        assert_eq!(session.phase(),Phase::Closing);assert_eq!(session.close_deadline(),Some(now+Duration::from_secs(5)));
        let ack=session.frame(MessageKind::Acknowledgement,0,&[0;32],&[],&[]).unwrap();
        session.receive(&ack).unwrap();assert!(session.shutdown_acknowledged());assert_eq!(session.phase(),Phase::Closing);
        assert!(session.peer_exited().as_slice().is_empty());assert_eq!(session.phase(),Phase::Closed);
    }
}

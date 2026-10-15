//! Bounded partial-I/O state. The existing process adapter supplies nonblocking
//! probes and cooperative readiness/deadline waits; no busy loop lives here.
use crate::wire::{Envelope,WireError};
#[derive(Debug,PartialEq,Eq)]
pub enum PumpError { Wire(WireError),Allocation,Busy,Closed,WriteCount }
impl From<WireError> for PumpError {fn from(error:WireError)->Self{Self::Wire(error)}}
#[derive(Debug,Clone,Copy,PartialEq,Eq)]
pub enum WriteProgress { Wait,Partial,Complete }
pub struct Pump {
    prefix:[u8;4],prefix_length:usize,incoming:Vec<u8>,body_length:Option<usize>,
    outgoing:Option<Vec<u8>>,written:usize,closed:bool,
}
impl Default for Pump {
    fn default()->Self {Self{prefix:[0;4],prefix_length:0,incoming:Vec::new(),body_length:None,outgoing:None,written:0,closed:false}}
}
impl Pump {
    /// Consumes at most one frame and returns the consumed input byte count.
    /// The caller retains any remaining probe bytes for the next invocation.
    pub fn feed(&mut self,input:&[u8])->Result<(usize,Option<Vec<u8>>),PumpError> {
        let result=self.feed_inner(input);
        if result.is_err() {self.close();}
        result
    }
    fn feed_inner(&mut self,input:&[u8])->Result<(usize,Option<Vec<u8>>),PumpError> {
        if self.closed {return Err(PumpError::Closed)}
        let mut consumed=0;
        if self.prefix_length<4 {
            let count=(4-self.prefix_length).min(input.len());
            self.prefix[self.prefix_length..self.prefix_length+count].copy_from_slice(&input[..count]);
            consumed+=count;self.prefix_length+=count;
            if self.prefix_length<4 {return Ok((consumed,None))}
            let length=u32::from_le_bytes(self.prefix) as usize;
            if length>16*1024*1024 {return Err(WireError::Limit.into())}
            if length<64 {return Err(WireError::Truncated.into())}
            self.incoming.try_reserve_exact(length+4).map_err(|_|PumpError::Allocation)?;
            self.incoming.extend_from_slice(&self.prefix);self.body_length=Some(length);
        }
        let length=self.body_length.ok_or(PumpError::Closed)?+4;
        let count=(length-self.incoming.len()).min(input.len()-consumed);
        self.incoming.extend_from_slice(&input[consumed..consumed+count]);consumed+=count;
        if self.incoming.len()<length {return Ok((consumed,None))}
        Envelope::decode_frame(&self.incoming)?;
        self.prefix_length=0;self.body_length=None;
        Ok((consumed,Some(std::mem::take(&mut self.incoming))))
    }
    /// EOF while a prefix/body is incomplete is a terminal truncation. A zero
    /// progress nonblocking probe must use cooperative Wait instead of EOF.
    pub fn eof(&mut self)->Result<(),PumpError> {
        let incomplete=self.prefix_length!=0 || self.body_length.is_some();self.close();
        if incomplete {Err(WireError::Truncated.into())}else{Ok(())}
    }
    /// One owned outgoing frame only; this caps retained send storage without
    /// an unbounded per-request queue. Session waiters carry no payload copies.
    pub fn queue(&mut self,frame:Vec<u8>)->Result<(),PumpError> {
        if self.closed {return Err(PumpError::Closed)}
        if self.outgoing.is_some() {return Err(PumpError::Busy)}
        Envelope::decode_frame(&frame)?;
        self.written=0;self.outgoing=Some(frame);Ok(())
    }
    pub fn output(&self)->Option<&[u8]> {self.outgoing.as_ref().map(|frame|&frame[self.written..])}
    pub fn wrote(&mut self,count:usize)->Result<WriteProgress,PumpError> {
        if self.closed {return Err(PumpError::Closed)}
        let Some(frame)=self.outgoing.as_ref() else{return Err(PumpError::WriteCount)};
        if count>frame.len()-self.written {self.close();return Err(PumpError::WriteCount)}
        if count==0 {return Ok(WriteProgress::Wait)}
        self.written+=count;
        if self.written==frame.len() {self.outgoing=None;self.written=0;Ok(WriteProgress::Complete)}
        else {Ok(WriteProgress::Partial)}
    }
    pub fn close(&mut self) {self.incoming=Vec::new();self.outgoing=None;self.body_length=None;self.prefix_length=0;self.closed=true;}
}
#[cfg(test)] mod tests {
    use super::*;
    fn frame()->Vec<u8>{Envelope{kind:crate::wire::MessageKind::Request,generation:1,request:1,library:"peer",binding:&[1;32],shapes:&[],payload:&[0]}.encode_frame().unwrap()}
    #[test] fn every_fragment_boundary_and_following_frame_survive() {
        let frame=frame();
        for boundary in 0..frame.len() {
            let mut pump=Pump::default();assert_eq!(pump.feed(&frame[..boundary]).unwrap(),(boundary,None));
            let mut rest=frame[boundary..].to_vec();rest.extend_from_slice(&frame);
            let (consumed,value)=pump.feed(&rest).unwrap();assert_eq!(consumed,frame.len()-boundary);assert_eq!(value.unwrap(),frame);
            assert_eq!(pump.feed(&rest[consumed..]).unwrap(),(frame.len(),Some(frame.clone())));
            assert!(pump.eof().is_ok());
        }
    }
    #[test] fn partial_writes_and_zero_progress_preserve_exact_frame() {
        let frame=frame();let mut pump=Pump::default();pump.queue(frame.clone()).unwrap();
        assert_eq!(pump.queue(frame.clone()),Err(PumpError::Busy));
        assert_eq!(pump.wrote(0).unwrap(),WriteProgress::Wait);assert_eq!(pump.output().unwrap(),frame);
        assert_eq!(pump.wrote(3).unwrap(),WriteProgress::Partial);assert_eq!(pump.output().unwrap(),&frame[3..]);
        assert_eq!(pump.wrote(frame.len()-3).unwrap(),WriteProgress::Complete);assert!(pump.output().is_none());
    }
    #[test] fn oversized_prefix_and_truncated_eof_fail_closed() {
        let mut pump=Pump::default();assert_eq!(pump.feed(&(16u32*1024*1024+1).to_le_bytes()),Err(PumpError::Wire(WireError::Limit)));
        assert_eq!(pump.feed(&[]),Err(PumpError::Closed));
        let mut pump=Pump::default();pump.feed(&frame()[..5]).unwrap();assert_eq!(pump.eof(),Err(PumpError::Wire(WireError::Truncated)));
        let mut pump=Pump::default();pump.queue(frame()).unwrap();assert_eq!(pump.wrote(usize::MAX),Err(PumpError::WriteCount));assert!(pump.output().is_none());
    }
}

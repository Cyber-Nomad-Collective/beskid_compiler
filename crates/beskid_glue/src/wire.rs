//! Version-one stdio envelope. Payload bytes are supplied by the shared value
//! format adapter; envelope identities never confer native image authority.
const MAX_BODY: usize = 16 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError { Truncated, Version, Kind, Flags, Identity, Limit, Utf8, Trailing, Allocation }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageKind { Handshake, Request, Response, Error, Cancel, Shutdown, Acknowledgement }
impl TryFrom<u8> for MessageKind {
    type Error = WireError;
    fn try_from(value:u8)->Result<Self,WireError> { Ok(match value {
        0=>Self::Handshake,1=>Self::Request,2=>Self::Response,3=>Self::Error,
        4=>Self::Cancel,5=>Self::Shutdown,6=>Self::Acknowledgement,_=>return Err(WireError::Kind),
    }) }
}
#[derive(Debug, PartialEq, Eq)]
pub struct Envelope<'a> {
    pub kind: MessageKind,
    pub generation:u64,
    pub request:u64,
    pub library:&'a str,
    pub binding:&'a [u8;32],
    /// Contiguous ordered SHA-256 identities. Every digest is exactly 32 bytes.
    pub shapes:&'a [u8],
    pub payload:&'a [u8],
}
struct Cursor<'a> { bytes:&'a [u8], offset:usize }
impl<'a> Cursor<'a> {
    fn take(&mut self,length:usize)->Result<&'a [u8],WireError> {
        let end=self.offset.checked_add(length).ok_or(WireError::Limit)?;
        let value=self.bytes.get(self.offset..end).ok_or(WireError::Truncated)?;
        self.offset=end; Ok(value)
    }
    fn u32(&mut self)->Result<u32,WireError> { Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap())) }
    fn u64(&mut self)->Result<u64,WireError> { Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap())) }
}
impl Envelope<'_> {
    fn validate(&self)->Result<(),WireError> {
        if self.generation==0 || self.library.is_empty() || self.library.as_bytes().contains(&0) {return Err(WireError::Identity)}
        if self.library.len()>4096 || self.shapes.len()%32!=0 || self.shapes.len()/32>256 || self.payload.len()>MAX_BODY {return Err(WireError::Limit)}
        match self.kind {
            MessageKind::Handshake|MessageKind::Shutdown => {
                if self.request!=0 || *self.binding!=[0;32] || !self.shapes.is_empty() {return Err(WireError::Identity)}
            },
            MessageKind::Request|MessageKind::Response|MessageKind::Error|MessageKind::Cancel => {
                if self.request==0 || *self.binding==[0;32] {return Err(WireError::Identity)}
            },
            MessageKind::Acknowledgement => {
                if self.request==0 && (*self.binding!=[0;32] || !self.shapes.is_empty()) {return Err(WireError::Identity)}
            },
        }
        Ok(())
    }
    /// One complete frame. Partial I/O belongs to the cooperative pump, which
    /// must bound the prefix before buffering; this parser allocates nothing.
    pub fn decode_frame(frame:&[u8])->Result<Envelope<'_>,WireError> {
        let mut cursor=Cursor{bytes:frame,offset:0};
        let length=cursor.u32()? as usize;
        if length>MAX_BODY {return Err(WireError::Limit)}
        let body=cursor.take(length)?;
        if cursor.offset!=frame.len() {return Err(WireError::Trailing)}
        let mut cursor=Cursor{bytes:body,offset:0};
        if cursor.take(2)?!=1u16.to_le_bytes() {return Err(WireError::Version)}
        let kind=MessageKind::try_from(cursor.take(1)?[0])?;
        if cursor.take(1)?[0]!=0 {return Err(WireError::Flags)}
        let generation=cursor.u64()?; let request=cursor.u64()?;
        let library_len=cursor.u32()? as usize;
        if library_len>4096 {return Err(WireError::Limit)}
        let library=std::str::from_utf8(cursor.take(library_len)?).map_err(|_|WireError::Utf8)?;
        let binding=cursor.take(32)?.try_into().unwrap();
        let shape_count=cursor.u32()? as usize;
        if shape_count>256 {return Err(WireError::Limit)}
        let shapes=cursor.take(shape_count*32)?;
        let payload_len=cursor.u32()? as usize;
        let payload=cursor.take(payload_len)?;
        if cursor.offset!=body.len() {return Err(WireError::Trailing)}
        let result=Envelope{kind,generation,request,library,binding,shapes,payload};
        result.validate()?; Ok(result)
    }
    pub fn encode_frame(&self)->Result<Vec<u8>,WireError> {
        self.validate()?;
        let length=64usize.checked_add(self.library.len()).and_then(|n|n.checked_add(self.shapes.len()))
            .and_then(|n|n.checked_add(self.payload.len())).ok_or(WireError::Limit)?;
        if length>MAX_BODY {return Err(WireError::Limit)}
        let mut bytes=Vec::new();
        bytes.try_reserve_exact(length+4).map_err(|_|WireError::Allocation)?;
        bytes.extend_from_slice(&(length as u32).to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes()); bytes.extend_from_slice(&[self.kind as u8,0]);
        bytes.extend_from_slice(&self.generation.to_le_bytes()); bytes.extend_from_slice(&self.request.to_le_bytes());
        bytes.extend_from_slice(&(self.library.len() as u32).to_le_bytes()); bytes.extend_from_slice(self.library.as_bytes());
        bytes.extend_from_slice(self.binding); bytes.extend_from_slice(&((self.shapes.len()/32) as u32).to_le_bytes());
        bytes.extend_from_slice(self.shapes); bytes.extend_from_slice(&(self.payload.len() as u32).to_le_bytes());
        bytes.extend_from_slice(self.payload); Ok(bytes)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn exact_wire_prefix_and_borrowed_roundtrip() {
        let binding=[3;32]; let shapes=[5;64]; let payload=[0,255,1];
        let value=Envelope{kind:MessageKind::Request,generation:7,request:9,library:"rust/λ",binding:&binding,shapes:&shapes,payload:&payload};
        let bytes=value.encode_frame().unwrap();
        assert_eq!(&bytes[4..8],&[1,0,1,0]);
        assert_eq!(&bytes[8..16],&7u64.to_le_bytes());
        assert_eq!(Envelope::decode_frame(&bytes).unwrap(),value);
        for end in 0..bytes.len() {assert!(Envelope::decode_frame(&bytes[..end]).is_err());}
        let mut extra=bytes.clone();extra.push(0);assert_eq!(Envelope::decode_frame(&extra),Err(WireError::Trailing));
    }
    #[test] fn reject_before_payload_allocation_or_dispatch() {
        let zero=[0;32];
        let value=Envelope{kind:MessageKind::Handshake,generation:1,request:0,library:"rust",binding:&zero,shapes:&[],payload:&[]};
        let bytes=value.encode_frame().unwrap();
        for (offset,value,error) in [(6,7,WireError::Kind),(7,1,WireError::Flags)] {
            let mut bad=bytes.clone();bad[offset]=value;assert_eq!(Envelope::decode_frame(&bad),Err(error));
        }
        assert_eq!(Envelope::decode_frame(&((MAX_BODY+1) as u32).to_le_bytes()),Err(WireError::Limit));
        let mut bad=bytes.clone();bad[24..28].copy_from_slice(&4097u32.to_le_bytes());
        assert_eq!(Envelope::decode_frame(&bad),Err(WireError::Limit));
        let mut bad=bytes;bad[28]=255;assert_eq!(Envelope::decode_frame(&bad),Err(WireError::Utf8));
    }
}

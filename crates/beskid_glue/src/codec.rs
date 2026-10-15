//! Glue binary value adapter. Values describe wire data, never native authority.
use beskid_serialization::{DataValue, Decoder, Encoder, Error, Limits, SequenceKind};

pub fn glue_limits()->Limits { Limits { max_input_bytes:16*1024*1024,max_output_bytes:16*1024*1024,
    max_total_bytes:16*1024*1024,max_scalar_bytes:16*1024*1024,max_nodes:1_048_576,max_depth:64 } }
#[derive(Default)]
pub struct BinaryEncoder { bytes:Vec<u8>, limits:Option<Limits>, written:bool }
impl BinaryEncoder {
    fn put(&mut self,bytes:&[u8])->Result<(),Error> {
        let limit=self.limits.ok_or(Error::InvalidValue("encoder not begun"))?.max_output_bytes;
        if bytes.len()>limit.saturating_sub(self.bytes.len()) {return Err(Error::Limit("output bytes"))}
        self.bytes.try_reserve(bytes.len()).map_err(|_|Error::AllocationFailure)?;
        self.bytes.extend_from_slice(bytes);Ok(())
    }
    fn count(&mut self,count:usize)->Result<(),Error> {
        self.put(&u32::try_from(count).map_err(|_|Error::Limit("wire length"))?.to_le_bytes())
    }
    fn blob(&mut self,bytes:&[u8])->Result<(),Error> {self.count(bytes.len())?;self.put(bytes)}
    fn values(&mut self,values:&[DataValue],depth:usize)->Result<(),Error> {
        self.count(values.len())?;for value in values {self.value(value,depth+1)?;}Ok(())
    }
    fn fields(&mut self,fields:&[(String,DataValue)],depth:usize)->Result<(),Error> {
        self.count(fields.len())?;for (name,value) in fields {self.blob(name.as_bytes())?;self.value(value,depth+1)?;}Ok(())
    }
    fn value(&mut self,value:&DataValue,depth:usize)->Result<(),Error> {
        if depth>64 {return Err(Error::Limit("depth"))}
        match value {
            DataValue::Unit=>self.put(&[0]),DataValue::Boolean(value)=>self.put(&[1,u8::from(*value)]),
            DataValue::Signed{value,width}=>{self.put(&[2,*width])?;self.put(&value.to_le_bytes())},
            DataValue::Unsigned{value,width}=>{self.put(&[3,*width])?;self.put(&value.to_le_bytes())},
            DataValue::Float{bits,width}=>{self.put(&[4,*width])?;self.put(&bits.to_le_bytes())},
            DataValue::Scalar(value)=>{self.put(&[5])?;self.put(&(*value as u32).to_le_bytes())},
            DataValue::String(value)=>{self.put(&[6])?;self.blob(value.as_bytes())},
            DataValue::Bytes(value)=>{self.put(&[7])?;self.blob(value)},
            DataValue::Sequence{kind,values}=>{self.put(&[if *kind==SequenceKind::Array {8}else{9}])?;self.values(values,depth)},
            DataValue::Record{shape,fields}=>{self.put(&[10])?;self.put(shape)?;self.fields(fields,depth)},
            DataValue::Map(fields)=>{self.put(&[11])?;self.fields(fields,depth)},
            DataValue::Variant{shape,name,payload}=>{self.put(&[12])?;self.put(shape)?;self.blob(name.as_bytes())?;self.values(payload,depth)},
            DataValue::Optional{present,payload}=>{self.put(&[13,u8::from(*present)])?;for value in payload {self.value(value,depth+1)?;}Ok(())},
        }
    }
}
impl Encoder for BinaryEncoder {
    fn begin(&mut self,limits:&Limits)->Result<(),Error> {self.abort();self.limits=Some(*limits);Ok(())}
    fn write_value(&mut self,value:&DataValue)->Result<(),Error> {
        let result=(|| {
            if self.written {return Err(Error::InvalidValue("multiple root values"))}
            let limits=self.limits.ok_or(Error::InvalidValue("encoder not begun"))?;
            beskid_serialization::validate(value,&limits)?;
            self.written=true;self.value(value,0)
        })();
        if result.is_err() {self.abort();}
        result
    }
    fn commit(&mut self)->Result<Vec<u8>,Error> {
        if !self.written || self.limits.is_none() {return Err(Error::InvalidValue("missing root value"))}
        self.limits=None;self.written=false;Ok(std::mem::take(&mut self.bytes))
    }
    fn abort(&mut self) {self.bytes.clear();self.limits=None;self.written=false;}
}
#[derive(Default)]
pub struct BinaryDecoder;
struct Reader<'a> { input:&'a [u8],offset:usize,limits:&'a Limits,nodes:usize,reserved_nodes:usize,scalar_bytes:usize }
impl<'a> Reader<'a> {
    fn take(&mut self,count:usize)->Result<&'a [u8],Error> {
        let end=self.offset.checked_add(count).ok_or(Error::Limit("input bytes"))?;
        let result=self.input.get(self.offset..end).ok_or(Error::InvalidValue("truncated value"))?;
        self.offset=end;Ok(result)
    }
    fn byte(&mut self)->Result<u8,Error> {Ok(self.take(1)?[0])}
    fn count(&mut self)->Result<usize,Error> {Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()) as usize)}
    fn blob(&mut self)->Result<Vec<u8>,Error> {
        let count=self.count()?;
        if count>self.limits.max_scalar_bytes || count>self.limits.max_total_bytes.saturating_sub(self.scalar_bytes) {return Err(Error::Limit("scalar bytes"))}
        self.scalar_bytes+=count;
        let input=self.take(count)?;let mut bytes=Vec::new();
        bytes.try_reserve_exact(count).map_err(|_|Error::AllocationFailure)?;bytes.extend_from_slice(input);Ok(bytes)
    }
    fn text(&mut self)->Result<String,Error> {String::from_utf8(self.blob()?).map_err(|_|Error::InvalidValue("UTF8"))}
    fn reserve<T>(&mut self,count:usize)->Result<Vec<T>,Error> {
        // Debit promised children across every open aggregate. Counting only
        // visited nodes permits nested wide arrays to reserve the same budget
        // repeatedly before the first branch reaches its leaves.
        if count>self.limits.max_nodes.saturating_sub(self.reserved_nodes) {return Err(Error::Limit("nodes"))}
        // Every child requires at least one wire tag. Reject impossible counts
        // before reserving their potentially much larger in-memory records.
        if count>self.input.len().saturating_sub(self.offset) {return Err(Error::InvalidValue("truncated collection"))}
        self.reserved_nodes+=count;
        let mut result=Vec::new();result.try_reserve_exact(count).map_err(|_|Error::AllocationFailure)?;Ok(result)
    }
    fn values(&mut self,depth:usize)->Result<Vec<DataValue>,Error> {
        let count=self.count()?;let mut values=self.reserve(count)?;
        for _ in 0..count {values.push(self.value(depth+1)?);}Ok(values)
    }
    fn fields(&mut self,depth:usize)->Result<Vec<(String,DataValue)>,Error> {
        let count=self.count()?;let mut fields=self.reserve(count)?;
        for _ in 0..count {
            let name=self.text()?;
            let value=self.value(depth+1)?;fields.push((name,value));
        }Ok(fields)
    }
    fn value(&mut self,depth:usize)->Result<DataValue,Error> {
        if depth>self.limits.max_depth.min(64) {return Err(Error::Limit("depth"))}
        self.nodes=self.nodes.checked_add(1).ok_or(Error::Limit("nodes"))?;
        if self.nodes>self.limits.max_nodes {return Err(Error::Limit("nodes"))}
        Ok(match self.byte()? {
            0=>DataValue::Unit,
            1=>DataValue::Boolean(match self.byte()? {0=>false,1=>true,_=>return Err(Error::InvalidValue("boolean"))}),
            tag@2..=4=>{
                let width=self.byte()?;let bytes:[u8;8]=self.take(8)?.try_into().unwrap();
                match tag {
                    2=>DataValue::Signed{value:i64::from_le_bytes(bytes),width},
                    3=>DataValue::Unsigned{value:u64::from_le_bytes(bytes),width},
                    _=>DataValue::Float{bits:u64::from_le_bytes(bytes),width},
                }
            },
            5=>DataValue::Scalar(char::from_u32(u32::from_le_bytes(self.take(4)?.try_into().unwrap())).ok_or(Error::InvalidValue("Unicode scalar"))?),
            6=>DataValue::String(self.text()?),7=>DataValue::Bytes(self.blob()?),
            tag@8..=9=>DataValue::Sequence{kind:if tag==8 {SequenceKind::Array}else{SequenceKind::List},values:self.values(depth)?},
            10=>{let shape=self.take(32)?.try_into().unwrap();DataValue::Record{shape,fields:self.fields(depth)?}},
            11=>DataValue::Map(self.fields(depth)?),
            12=>{let shape=self.take(32)?.try_into().unwrap();let name=self.text()?;DataValue::Variant{shape,name,payload:self.values(depth)?}},
            13=>{let present=match self.byte()? {0=>false,1=>true,_=>return Err(Error::InvalidValue("optional"))};
                let mut payload=self.reserve(usize::from(present))?;if present {payload.push(self.value(depth+1)?);}DataValue::Optional{present,payload}},
            _=>return Err(Error::InvalidValue("wire tag")),
        })
    }
}
impl Decoder for BinaryDecoder {
    fn read_value(&mut self,input:&[u8],limits:&Limits)->Result<DataValue,Error> {
        if input.len()>limits.max_input_bytes {return Err(Error::Limit("input bytes"))}
        let mut reader=Reader{input,offset:0,limits,nodes:0,reserved_nodes:1,scalar_bytes:0};
        let value=reader.value(0)?;
        if reader.offset!=input.len() {return Err(Error::InvalidValue("trailing bytes"))}
        beskid_serialization::validate(&value,limits)?;Ok(value)
    }
}
#[cfg(test)] mod tests {
    use super::*;
    fn roundtrip(value:DataValue) {
        let bytes=beskid_serialization::encode(&value,&mut BinaryEncoder::default(),&glue_limits()).unwrap();
        assert_eq!(beskid_serialization::decode(&bytes,&mut BinaryDecoder,&glue_limits()).unwrap(),value);
        for end in 0..bytes.len() {assert!(beskid_serialization::decode(&bytes[..end],&mut BinaryDecoder,&glue_limits()).is_err());}
        let mut extra=bytes;extra.push(0);assert!(beskid_serialization::decode(&extra,&mut BinaryDecoder,&glue_limits()).is_err());
    }
    #[test] fn full_primitive_bits_and_boundaries() {
        for width in [8,16,32,64] {
            let max=if width==64 {i64::MAX}else{(1i64<<(width-1))-1};
            for value in [-max-1,0,max] {roundtrip(DataValue::Signed{value,width});}
            roundtrip(DataValue::Unsigned{value:if width==64 {u64::MAX}else{(1u64<<width)-1},width});
        }
        for bits in [0,0x80000000,0x7f800000,0x7fc00001,0xffc00002] {roundtrip(DataValue::Float{bits,width:32});}
        for bits in [0,0x8000000000000000,0x7ff0000000000000,0x7ff8000000000001,0xfff8000000000002] {roundtrip(DataValue::Float{bits,width:64});}
        for value in [DataValue::Unit,DataValue::Boolean(false),DataValue::Boolean(true),DataValue::Scalar('😀'),
            DataValue::String("a\0λ😀".into()),DataValue::Bytes(vec![0,255,128])] {roundtrip(value);}
    }
    #[test] fn composites_preserve_order_without_capability_grants() {
        roundtrip(DataValue::Record{shape:[4;32],fields:vec![("x".into(),DataValue::Sequence{kind:SequenceKind::List,values:vec![DataValue::Optional{present:true,payload:vec![DataValue::Variant{shape:[9;32],name:"Some".into(),payload:vec![DataValue::Bytes(vec![1,2])]}]}]})]});
        roundtrip(DataValue::Map(vec![("z".into(),DataValue::Unit),("a".into(),DataValue::Boolean(true))]));
    }
    #[test] fn malformed_values_and_preallocation_budget_reject() {
        for bytes in [&[1,2][..],&[5,0,216,0,0][..],&[6,1,0,0,0,255][..],&[13,2][..],&[99][..]] {
            assert!(beskid_serialization::decode(bytes,&mut BinaryDecoder,&glue_limits()).is_err());
        }
        let mut limits=glue_limits();limits.max_nodes=2;
        assert_eq!(beskid_serialization::decode(&[8,255,255,255,127],&mut BinaryDecoder,&limits),Err(Error::Limit("nodes")));
        limits.max_nodes=5;
        // Parent promises three children, then its first child tries to reserve
        // another three while the two pending siblings still own their budget.
        assert_eq!(beskid_serialization::decode(&[8,3,0,0,0,8,3,0,0,0,0,0,0,0,0],&mut BinaryDecoder,&limits),Err(Error::Limit("nodes")));
        limits.max_scalar_bytes=1;
        assert_eq!(beskid_serialization::decode(&[7,2,0,0,0,1,2],&mut BinaryDecoder,&limits),Err(Error::Limit("scalar bytes")));
        let mut encoder=BinaryEncoder::default();limits.max_output_bytes=1;
        assert!(beskid_serialization::encode(&DataValue::Bytes(vec![1]),&mut encoder,&limits).is_err());
        assert!(encoder.commit().is_err());
        encoder.begin(&limits).unwrap();
        assert!(encoder.write_value(&DataValue::Bytes(vec![1])).is_err());
        assert!(encoder.commit().is_err());
    }
}

//! Bounded constructor input from current private shape/catchall carriers.
//!
//! The recipe describes the serialization wire view the generated adapters
//! write and read. Field facts come from the Mod's field policy, read through
//! `beskid_queries::serialization_field_policy`: the effective wire name,
//! direction-aware requiredness, the explicit `word` wire width and the bytes
//! selection. Fields skipped in both directions are already absent from the
//! issued wire graph.
use crate::CodegenInput;
use beskid_queries::{
    AstNodeKey, CanonicalContainerKind, DynamicPackingField, DynamicPackingNode, DynamicPackingShape,
    SerializationShapeBinding,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
#[derive(Debug,Clone)]
pub struct CompiledDescriptorRecipe { source:String, root:[u8;32], extras:Option<CompiledExtrasRecipe> }
#[derive(Debug,Clone)]
pub struct CompiledExtrasRecipe { member:String, map:String, value:String, signature:String }
impl CompiledExtrasRecipe {
 pub fn member(&self)->&str{&self.member} pub fn map(&self)->&str{&self.map}
 pub fn value(&self)->&str{&self.value} pub fn signature(&self)->&str{&self.signature}
}
impl CompiledDescriptorRecipe { pub fn source(&self)->&str {&self.source} pub fn root(&self)->&[u8;32] {&self.root} pub fn extras(&self)->Option<&CompiledExtrasRecipe>{self.extras.as_ref()} }
struct Writer {source:String,limit:usize}
impl Writer {
 fn token(&mut self,value:&str)->Result<(),String> {
  let length=value.len().to_string();
  if value.len()>1048576 || self.source.len().checked_add(length.len()+1).and_then(|n|n.checked_add(value.len())).is_none_or(|n|n>self.limit) {return Err("compiled descriptor recipe limit".into())}
  self.source.push_str(&length);self.source.push(':');self.source.push_str(value);Ok(())
 }
 fn number(&mut self,value:usize)->Result<(),String>{self.token(&value.to_string())}
}
fn hex(bytes:&[u8])->String{bytes.iter().map(|byte|format!("{byte:02x}")).collect()}

/// One descriptor node per distinct wire signature. Node 0 is the root.
struct Emitter<'a,'b> {
 input:&'a CodegenInput<'b>,
 schema:&'a str,
 max_bytes:usize,
 signature_bytes:usize,
 slots:HashMap<String,usize>,
 signatures:HashMap<String,String>,
 chunks:Vec<Option<String>>,
 catchall:Option<AstNodeKey>,
}
impl Emitter<'_,'_> {
 fn charge(&mut self,signature:&str)->Result<(),String>{
  self.signature_bytes=self.signature_bytes.checked_add(signature.len()).filter(|bytes|*bytes<=self.max_bytes).ok_or("compiled descriptor signature limit")?;Ok(())
 }
 fn remaining(&self)->usize{self.max_bytes.saturating_sub(self.signature_bytes).min(1048576)}
 /// Reserve a slot for a new signature, or return the existing node id.
 fn reserve(&mut self,signature:String)->Result<(String,Option<usize>),String>{
  let id=hex(&Sha256::digest(signature.as_bytes()));
  if self.slots.contains_key(&id){return Ok((id,None))}
  self.charge(&signature)?;
  let slot=self.chunks.len();self.chunks.push(None);self.slots.insert(id.clone(),slot);self.signatures.insert(id.clone(),signature);
  Ok((id,Some(slot)))
 }
 fn header(&self,id:&str,declaration:Option<AstNodeKey>)->Result<Writer,String>{
  let mut writer=Writer{source:String::new(),limit:self.max_bytes};
  writer.token(id)?;writer.token(&self.signatures[id])?;
  if let Some(declaration)=declaration {
   let identity=beskid_queries::portable_nominal_identity(self.input.database(),self.input.typed_program(),declaration).map_err(|e|e.to_string())?;
   // These readonly fields are descriptive. Full identity authority stays in
   // the signature's length-framed source-kind/registry/artifact tokens.
   writer.token(&format!("{}@{}:{}",identity.package().package_name(),identity.package().version(),identity.package().source_digest()))?;
   writer.token(&identity.declaration().source_path)?;writer.token(&identity.declaration().lexical_path.join("."))?;
  }else{writer.token("")?;writer.token("")?;writer.token("")?;}
  writer.token(self.schema)?;Ok(writer)
 }
 /// Explicit bytes: a distinct wire signature, never the `u8[]` sequence node.
 fn bytes(&mut self)->Result<String,String>{
  let mut signature=Writer{source:String::new(),limit:self.remaining()};
  signature.token("beskid.wire/1")?;signature.token(self.schema)?;signature.token("bytes")?;
  let (id,slot)=self.reserve(signature.source)?;
  if let Some(slot)=slot {
   let mut writer=self.header(&id,None)?;
   writer.token("bytes")?;writer.number(0)?;writer.number(0)?;writer.number(0)?;writer.number(0)?;
   self.chunks[slot]=Some(writer.source);
  }
  Ok(id)
 }
 /// The wire shape of one field value under its policy.
 fn field_shape(&mut self,graph:&DynamicPackingShape,ty:u32,policy:&beskid_queries::SerializationFieldPolicy)->Result<String,String>{
  if policy.bytes() {
   let Some(DynamicPackingNode::Array{element})=graph.nodes().get(ty as usize) else{return Err("SerializationBytesType: bytes requires u8[]".into())};
   if !matches!(graph.nodes().get(*element as usize),Some(DynamicPackingNode::Scalar{name:"u8",..})){return Err("SerializationBytesType: bytes requires u8[]".into())}
   return self.bytes();
  }
  if let Some(width)=policy.word_width() {
   let rewritten=graph.with_word_width(ty,width).map_err(|e|e.to_string())?;
   return self.node(&rewritten,0);
  }
  self.node(graph,ty)
 }
 fn fields(&mut self,writer:&mut Writer,graph:&DynamicPackingShape,fields:&[DynamicPackingField],payload:bool)->Result<(),String>{
  let fields=fields.iter().filter(|field|Some(field.declaration)!=self.catchall).collect::<Vec<_>>();
  let mut entries=Vec::with_capacity(fields.len());
  for field in fields {
   let database=self.input.database();
   let policy=if payload {beskid_queries::serialization_payload_field_policy(database,field.declaration)} else {beskid_queries::serialization_field_policy(database,field.declaration)}.map_err(|e|e.to_string())?;
   let shape=self.field_shape(graph,field.ty,&policy)?;
   let optional=matches!(graph.nodes().get(field.ty as usize),Some(DynamicPackingNode::Enum{container:Some(CanonicalContainerKind::Optional),..}));
   // A record field is required only when both directions carry it: a
   // SkipSerialize field is decode-only and a SkipDeserialize field is
   // encode-only. Payload fields here are on the wire in both directions.
   let required=!optional && (payload || policy.required_in_both_directions());
   entries.push((policy.wire_name().to_owned(),shape,required,optional));
  }
  writer.number(entries.len())?;
  for (name,shape,required,optional) in entries {
   writer.token(&name)?;writer.token(&shape)?;writer.token(if required{"1"}else{"0"})?;writer.token(if optional{"1"}else{"0"})?;writer.token("")?;
  }
  Ok(())
 }
 fn node(&mut self,graph:&DynamicPackingShape,index:u32)->Result<String,String>{
  let rooted=graph.rooted_at(index).map_err(|e|e.to_string())?;
  let compiled=self.input.compile_stable_shape_graph(&rooted,self.schema,self.remaining())?;
  let signature=String::from_utf8(compiled.signature().to_vec()).map_err(|_|"nonUTF8 issued signature")?;
  let (id,slot)=self.reserve(signature)?;
  let Some(slot)=slot else{return Ok(id)};
  let node=graph.nodes().get(index as usize).ok_or("descriptor node outside graph")?;
  let declaration=match node{DynamicPackingNode::Record{declaration,..}|DynamicPackingNode::Enum{declaration,..}=>Some(*declaration),_=>None};
  let (kind,width,args)=match node {
   DynamicPackingNode::Scalar{name,..}=>{
    let (kind,width)=match *name{"unit"=>("unit",0),"bool"=>("bool",0),"char"=>("scalar",0),"utf8"=>("string",0),"i8"=>("signed",8),"i16"=>("signed",16),"i32"=>("signed",32),"i64"=>("signed",64),"u8"=>("unsigned",8),"u16"=>("unsigned",16),"u32"=>("unsigned",32),"u64"|"word"=>("unsigned",64),"f32"=>("float",32),"f64"=>("float",64),_=>return Err("unsupported issued scalar metadata".into())};(kind,width,vec![])
   },
   // `u8[]` is a sequence unless its field selects Bytes; see `field_shape`.
   DynamicPackingNode::Array{element}=>("array",0,vec![*element]),
   DynamicPackingNode::Record{container:Some(CanonicalContainerKind::List),arguments,..}=>("list",0,arguments.clone()),
   DynamicPackingNode::Record{container:Some(CanonicalContainerKind::Map),arguments,..}=>{
    if arguments.len()!=2 || !matches!(graph.nodes().get(arguments[0] as usize),Some(DynamicPackingNode::Scalar{name:"utf8",..})){return Err("compiled serialization Map requires string key".into())}("map",0,vec![arguments[1]])
   },
   DynamicPackingNode::Enum{container:Some(CanonicalContainerKind::Optional),arguments,..}=>("optional",0,arguments.clone()),
   DynamicPackingNode::Record{container:None,arguments,..}=>("record",0,arguments.clone()),
   DynamicPackingNode::Enum{container:None,arguments,..}=>("variant",0,arguments.clone()),
   _=>return Err("invalid canonical container body".into()),
  };
  let mut argument_ids=Vec::with_capacity(args.len());
  for argument in args {argument_ids.push(self.node(graph,argument)?);}
  let mut writer=self.header(&id,declaration)?;
  writer.token(kind)?;writer.number(width)?;writer.number(argument_ids.len())?;for argument in &argument_ids{writer.token(argument)?;}
  match node{DynamicPackingNode::Record{container:None,fields,..}=>self.fields(&mut writer,graph,fields,false)?,_=>writer.number(0)?};
  match node{DynamicPackingNode::Enum{container:None,variants,..}=>{writer.number(variants.len())?;for variant in variants{writer.token(&variant.name)?;writer.number(variant.ordinal as usize)?;self.fields(&mut writer,graph,&variant.fields,true)?;}},_=>writer.number(0)?};
  self.chunks[slot]=Some(writer.source);
  Ok(id)
 }
}

impl CodegenInput<'_> {
 pub fn compiled_descriptor_recipe(&self,binding:&SerializationShapeBinding,schema_version:&str,max_bytes:usize)->Result<CompiledDescriptorRecipe,String> {
  let graph=binding.graph();
  let root=match graph.nodes().first(){Some(DynamicPackingNode::Record{declaration,..}|DynamicPackingNode::Enum{declaration,..})=>*declaration,_=>return Err("compiled getter requires a current issued nominal target".into())};
  let shapes=self.compiled_serialization_shapes(schema_version,1048576)?;
  // A concrete target is a retained compiled shape. A template application is
  // bound to its retained template through the specialization gate instead.
  let (expected_root,extras_binding)=match shapes.iter().find(|shape|shape.owner()==root) {
   Some(shape)=>{
    // Compare actual resolved source body/arguments, not only a declaration or digest.
    let target=self.typed_program().assembly.compiled_mod_metadata.iter().filter_map(|m|m.issuer_payload::<beskid_queries::CompiledSerializationTarget>()).find_map(|target|target.rebind(self.database(),self.typed_program()).ok().filter(|current|current.owner()==root));
    let current=target.ok_or("compiled getter target correspondence absent")?;
    if current.shape(self.database(),self.typed_program()).map_err(|e|e.to_string())?.nodes()!=graph.nodes(){return Err("compiled getter application differs from retained target graph".into())}
    (Some(*shape.shape().sha256()),shape.extras().cloned())
   },
   None=>{
    beskid_queries::serialization_template_getter_shape(self.database(),self.typed_program(),binding).map_err(|e|e.to_string())?
     .ok_or("compiled getter lacks retained target contribution")?;
    (None,None)
   },
  };
  let mut catchall_field=None;
  let mut catchall_nodes=None;
  if let Some(binding)=&extras_binding {
   let Some(DynamicPackingNode::Record{fields:root_fields,..})=graph.nodes().first() else{return Err("catchall target is not a record".into())};
   let matches=root_fields.iter().filter(|field|field.declaration.unit.path(self.database())==&binding.field().source_unit
       && field.declaration.generation==binding.field().generation && field.declaration.node==binding.field().node).collect::<Vec<_>>();
   let [field]=matches.as_slice() else{return Err("catchall field correspondence outside issued graph".into())};
   let Some(DynamicPackingNode::Record{declaration,container:Some(CanonicalContainerKind::Map),arguments,..})=graph.nodes().get(field.ty as usize) else{return Err("catchall field lacks canonical Map graph".into())};
   if *declaration!=binding.map() || arguments.len()!=2 || !matches!(graph.nodes().get(arguments[0] as usize),Some(DynamicPackingNode::Scalar{name:"utf8",..})) {return Err("catchall map/value application differs".into())}
   catchall_field=Some(field.declaration);
   catchall_nodes=Some((field.name.clone(),field.ty,arguments[1]));
  }
  let mut emitter=Emitter{input:self,schema:schema_version,max_bytes,signature_bytes:0,slots:HashMap::new(),signatures:HashMap::new(),chunks:Vec::new(),catchall:catchall_field};
  let root_id=emitter.node(graph,0)?;
  if emitter.slots.get(&root_id)!=Some(&0){return Err("compiled descriptor root is not node zero".into())}
  let root_digest:[u8;32]=Sha256::digest(emitter.signatures[&root_id].as_bytes()).into();
  if expected_root.is_some_and(|expected|expected!=root_digest){return Err("compiled getter root signature drift".into())}
  let extras=if let Some((member,map_node,value_node))=catchall_nodes {
   let map=emitter.node(graph,map_node)?;
   let value=emitter.node(graph,value_node)?;
   let mut proof=Writer{source:String::new(),limit:max_bytes};proof.token("beskid.extras/1")?;
   proof.token(&emitter.signatures[&root_id])?;
   proof.token(&member)?;
   proof.token(&emitter.signatures[&map])?;
   proof.token(&emitter.signatures[&value])?;
   Some(CompiledExtrasRecipe{member,map,value,signature:proof.source})
  } else {None};
  let mut writer=Writer{source:String::new(),limit:max_bytes};writer.token("beskid.descriptor/1")?;writer.number(emitter.chunks.len())?;
  for chunk in emitter.chunks {
   let chunk=chunk.ok_or("compiled descriptor node left unfinished")?;
   if writer.source.len().checked_add(chunk.len()).is_none_or(|total|total>max_bytes){return Err("compiled descriptor recipe limit".into())}
   writer.source.push_str(&chunk);
  }
  if writer.source.len().checked_add(extras.as_ref().map_or(0,|extras|extras.signature.len()+extras.member.len()+extras.map.len()+extras.value.len())).is_none_or(|total|total>max_bytes){return Err("compiled descriptor and extras aggregate limit".into())}
  Ok(CompiledDescriptorRecipe{source:writer.source,root:root_digest,extras})
 }
}

#[test]
fn append_initializes_the_element_before_publishing_its_owner() {
    let source = include_str!("../../src/context/calls/collections.rs");
    let append = source.split_once("CollectionOperation::Append { owner: mutation_owner }").unwrap().1;
    let append = append.split_once("CollectionOperation::Clear").unwrap().0;
    let grow = append.find("self.grow_array(owner, minimum)").unwrap();
    let element = append.find("store(MemFlagsData::new(), value, address, 0)").unwrap();
    let length = append.find("store(MemFlagsData::new(), next_length, array, length_offset)").unwrap();
    let publish = append.find("self.publish_collection_owner(").unwrap();
    assert!(grow < element && element < length && length < publish);
    assert!(!append.contains("call(barrier, &[base, array])"));
    assert!(!append.contains("call(barrier, &[owner, array])"));
}

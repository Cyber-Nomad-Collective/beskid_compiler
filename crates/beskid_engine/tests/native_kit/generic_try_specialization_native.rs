//! `?` on generic calls inside generic specializations, end to end on the canonical runtime kit:
//! a propagated generic call used as a method argument (`bag.Put(ReadOne<T, R>(reader, value)?)`)
//! and a propagated generic call whose parameter is only an explicit type argument
//! (`Admit<T>(limit)?`), including the propagated error path.

use std::path::Path;

use beskid_engine::services::run_entrypoint;

use crate::runtime_prefix::RuntimePrefixContext;
use crate::shared_kit;

const SOURCE: &str = r#"
enum Error { Failed() }
enum Result<TValue, TError> { Ok(TValue value), Error(TError error) }
contract Source { Result<i64, Error> Begin(i64 limit); }
type Reader : Source {
    i64 n,
    pub Result<i64, Error> Begin(i64 limit) { return Result::Ok(limit + n); }
}
type Bag<T> { T[] values, i64 count, Bag<T> Put(T next) { return Bag<T> { values: values, count: count + 1 }; } i64 Size() { return count; } }
T[] NoValues<T>() { return []; }
Bag<T> Empty<T>() { return Bag<T> { values: NoValues<T>(), count: 0 }; }
Result<T, Error> ReadOne<T, R>(R reader, T value) where R: Source { i64 begun = reader.Begin(1_i64)?; return Result::Ok(value); }
Result<Bag<T>, Error> Collect<T, R>(R reader, T value) where R: Source {
    Bag<T> bag = Empty<T>();
    return Result::Ok(bag.Put(ReadOne<T, R>(reader, value)?));
}
Result<unit, Error> Admit<T>(i64 limit) { if (limit < 0_i64) { return Result::Error(Error::Failed()); } return Result::Ok(()); }
Result<T, Error> Load<T>(T value, i64 limit) { Admit<T>(limit)?; return Result::Ok(value); }
type Point { i64 x }
type Boxed<T> { T item, T[] history }
enum Option<T> { Some(T value), None() }
type Holder<T> { T item }
Result<Holder<T>, Error> Assemble<T, R>(R reader, T value) where R: Source {
    mut Option<T> slot = Option<T>::None();
    slot = Option<T>::Some(ReadOne<T, R>(reader, value)?);
    return Result::Ok(Holder<T> { item: match slot { Option::Some(found) => found, Option::None() => { return Result::Error(Error::Failed()); }, } });
}
Result<i64, Error> CountAll<T>(T[] values) { return Result::Ok(1_i64); }
Result<i64, Error> Measure<T>(Boxed<T> value) {
    i64 counted = CountAll(value.history)?;
    i64 again = CountAll<T>((value).history)?;
    return Result::Ok(counted + again - 1_i64);
}
Result<T, Error> Complete<T, R>(R reader, T value) where R: Source {
    reader.Begin(1_i64)?;
    T loaded = ReadOne<T, R>(reader, value)?;
    reader.Begin(2_i64)?;
    return Result::Ok(loaded);
}
i64 Main() {
    Reader reader = Reader { n: 1_i64 };
    i64 a = match Collect<i64, Reader>(reader, 5_i64) { Result::Ok(_) => 1_i64, Result::Error(_) => -1_i64, };
    i64 b = match Load<i64>(40_i64, 1_i64) { Result::Ok(v) => v, Result::Error(_) => -100_i64, };
    i64 c = match Load<i64>(40_i64, -1_i64) { Result::Ok(_) => -1000_i64, Result::Error(_) => 1_i64, };
    i64 d = match Complete<i64, Reader>(reader, 2_i64) { Result::Ok(v) => v, Result::Error(_) => -1_i64, };
    i64 f = match Measure<i64>(Boxed<i64> { item: 1_i64, history: [1_i64] }) { Result::Ok(v) => v, Result::Error(_) => -1_i64, };
    i64 e = match Complete<Point, Reader>(reader, Point { x: -3_i64 }) { Result::Ok(p) => p.x, Result::Error(_) => -1_i64, };
    i64 g = match Assemble<i32, Reader>(reader, 5_i32) { Result::Ok(holder) => i64(holder.item), Result::Error(_) => -100_i64, };
    i64 h = match Assemble<Point, Reader>(reader, Point { x: -5_i64 }) { Result::Ok(holder) => holder.item.x, Result::Error(_) => -100_i64, };
    return a + b + c + d + e + f + g + h;
}
"#;

#[test]
fn propagated_generic_calls_run_inside_their_specializations() {
    let kit = shared_kit::debug();
    let _runtime_prefix = RuntimePrefixContext::install(kit.prefix());
    let output =
        run_entrypoint(Path::new("generic-try-specialization.bd"), SOURCE, "Main", &beskid_engine::ModHost::NoMods)
            .expect("propagated generic calls lower and run in their specializations");
    assert_eq!(output, "42", "one collected value, the loaded value and the propagated error");
}

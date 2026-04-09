use beskid_engine::Engine;
use beskid_runtime::{str_concat, str_len, str_new};

#[test]
fn runtime_string_concat_empty_and_ascii() {
    let mut engine = Engine::new();
    engine.with_arena(|_, _| {
        // Non-null pointer for empty: point at a 1-byte static buffer, len 0
        static Z: [u8; 1] = [0];
        let empty = str_new(Z.as_ptr(), 0);
        let hello = b"hello";
        let s1 = str_new(hello.as_ptr(), hello.len());

        let out1 = str_concat(empty, s1);
        assert_eq!(str_len(out1), 5);

        let out2 = str_concat(s1, empty);
        assert_eq!(str_len(out2), 5);
    });
}

#[test]
fn runtime_string_concat_utf8_multibyte() {
    let mut engine = Engine::new();
    engine.with_arena(|_, _| {
        let a = "\u{2713}".as_bytes(); // 3 bytes
        let b = "\u{1F916}".as_bytes(); // 4 bytes (🤖)
        let sa = str_new(a.as_ptr(), a.len());
        let sb = str_new(b.as_ptr(), b.len());

        let out = str_concat(sa, sb);
        assert_eq!(str_len(out), a.len() + b.len());
    });
}

#[test]
fn runtime_string_new_rejects_invalid_utf8() {
    let mut engine = Engine::new();
    engine.with_arena(|_, _| {
        let invalid = [0xffu8];
        let result = std::panic::catch_unwind(|| {
            let _ = str_new(invalid.as_ptr(), invalid.len());
        });
        assert!(result.is_err(), "expected invalid UTF-8 input to panic");
    });
}

#[test]
fn runtime_string_new_rejects_null_ptr() {
    let mut engine = Engine::new();
    engine.with_arena(|_, _| {
        let result = std::panic::catch_unwind(|| {
            let _ = str_new(std::ptr::null(), 0);
        });
        assert!(result.is_err(), "expected null string pointer to panic");
    });
}

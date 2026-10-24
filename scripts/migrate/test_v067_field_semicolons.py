import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import v067_field_semicolons as m  # noqa: E402


def _w(p, t):
    with open(p, "w") as fh:
        fh.write(t)


def _r(p):
    with open(p) as fh:
        return fh.read()


def rw(src):
    new, _ = m.rewrite_source(src)
    return new


class Fields(unittest.TestCase):
    def test_plain(self):
        self.assertEqual(rw("type Point { i32 x, i32 y }"), "type Point { i32 x; i32 y; }")

    def test_multiline_trailing_comma(self):
        self.assertEqual(rw("type P {\n    i32 x,\n    i32 y,\n}"), "type P {\n    i32 x;\n    i32 y;\n}")

    def test_multiline_no_trailing(self):
        self.assertEqual(rw("pub type P {\n    i32 x,\n    i32 y // end\n}"),
                         "pub type P {\n    i32 x;\n    i32 y; // end\n}")

    def test_generics(self):
        self.assertEqual(rw("type Pair<A, B> { Map<A, B> m, (A, B) => C f, Cb(A, B) g, A[] h }"),
                         "type Pair<A, B> { Map<A, B> m; (A, B) => C f; Cb(A, B) g; A[] h; }")
        self.assertEqual(rw("type X { List<Map<i32, string>> a, i32 b }"),
                         "type X { List<Map<i32, string>> a; i32 b; }")

    def test_conformance(self):
        self.assertEqual(rw("type X : A, B<C, D> { i32 a, i32 b }"), "type X : A, B<C, D> { i32 a; i32 b; }")

    def test_attributes(self):
        self.assertEqual(rw("type X { [Key(a, b)] i32 a, [Tag] pub i32 b }"),
                         "type X { [Key(a, b)] i32 a; [Tag] pub i32 b; }")

    def test_inject_and_event(self):
        src = "type S { inject Log log, inject global::Cfg cfg, event Done(i32 a, i32 b), event{4} Tick() }"
        self.assertEqual(rw(src),
                         "type S { inject Log log; inject global::Cfg cfg; event Done(i32 a, i32 b); event{4} Tick(); }")

    def test_comments_and_strings(self):
        src = "type X {\n    /// doc, with comma\n    i32 a, // note, here\n    /* x, y */ i32 b\n}"
        self.assertEqual(rw(src), "type X {\n    /// doc, with comma\n    i32 a; // note, here\n    /* x, y */ i32 b;\n}")
        src = 'type X { i32 a, i32 b }\nfn f() { g(",", \',\', 1, 2) }'
        self.assertEqual(rw(src), 'type X { i32 a; i32 b; }\nfn f() { g(",", \',\', 1, 2) }')

    def test_methods_untouched(self):
        src = "type X { i32 a, i32 b\n    i32 Sum(i32 p, i32 q) { return a + b } }"
        self.assertEqual(rw(src), "type X { i32 a; i32 b;\n    i32 Sum(i32 p, i32 q) { return a + b } }")
        src = "type X { i32 a,\n    i32 F<T>(T t, T u) where T: A, T: B => a; }"
        self.assertEqual(rw(src), "type X { i32 a;\n    i32 F<T>(T t, T u) where T: A, T: B => a; }")

    def test_binding(self):
        src = "type X : C { i32 a, type Item = i32; }"
        self.assertEqual(rw(src), "type X : C { i32 a; type Item = i32; }")

    def test_nested_in_module(self):
        src = "mod m {\n    type A { i32 x, i32 y }\n    type B { A a, A b }\n}"
        self.assertEqual(rw(src), "mod m {\n    type A { i32 x; i32 y; }\n    type B { A a; A b; }\n}")

    def test_empty_and_single(self):
        self.assertEqual(rw("type E {}"), "type E {}")
        self.assertEqual(rw("type S { i32 a }"), "type S { i32 a; }")

    def test_enum_literal_untouched(self):
        src = ("enum E { A(i32 x, i32 y), B, }\n"
               "fn f() { Point p = Point { x: 1, y: 2 }; g(1, 2) }\n"
               "fn h(i32 a, i32 b) => a;\ncontract C { type Item; i32 F(i32 a, i32 b); }\n"
               "extend type X { i32 G(i32 a, i32 b) => a; }\n")
        self.assertEqual(rw(src), src)

    def test_idempotent(self):
        src = "type A<T, U> {\n    [K(1, 2)] T a,\n    event E(i32 x, i32 y),\n    inject L l,\n}\n"
        once = rw(src)
        self.assertEqual(rw(once), once)
        self.assertNotEqual(once, src)

    def test_count(self):
        _, c = m.rewrite_source("type P { i32 x, i32 y }")
        self.assertEqual(c, 2)

    def test_unclassified_reported(self):
        rep = []
        src = "type P { i32 x, i32 y(, }"
        m.rewrite_source(src, rep, "f")
        self.assertTrue(rep)


class Markdown(unittest.TestCase):
    def test_blocks(self):
        src = ("Use `type P { i32 x, i32 y }` in prose, a, b.\n\n```beskid\ntype P { i32 x, i32 y }\n```\n\n"
               "```bd title=x\ntype Q {\n    i32 a,\n}\n```\n\n```rust\ntype P { i32 x, i32 y }\n```\n")
        exp = ("Use `type P { i32 x, i32 y }` in prose, a, b.\n\n```beskid\ntype P { i32 x; i32 y; }\n```\n\n"
               "```bd title=x\ntype Q {\n    i32 a;\n}\n```\n\n```rust\ntype P { i32 x, i32 y }\n```\n")
        new, c = m.rewrite_markdown(src)
        self.assertEqual(new, exp)
        self.assertEqual(c, 3)
        self.assertEqual(m.rewrite_markdown(new)[0], new)

    def test_tilde_and_longer_fence(self):
        src = "````beskid\n```\ntype P { i32 x, i32 y }\n````\n"
        self.assertEqual(m.rewrite_markdown(src)[0], "````beskid\n```\ntype P { i32 x; i32 y; }\n````\n")


class SameLine(unittest.TestCase):
    def test_type_after_semicolon_and_brace(self):
        self.assertEqual(
            rw("use Lib.A; use Lib.B; pub type Record { i32 value }"),
            "use Lib.A; use Lib.B; pub type Record { i32 value; }",
        )
        self.assertEqual(
            rw("enum E { A } type T { i32 a, i32 b } type U { T t, }"),
            "enum E { A } type T { i32 a; i32 b; } type U { T t; }",
        )


class Rust(unittest.TestCase):
    def rr(self, src):
        new, _ = m.rewrite_rust(src)
        return new

    def test_plain_string_with_escapes(self):
        src = 'let s = "type P {\\n    i32 x,\\n    string s,\\n}\\n";'
        self.assertEqual(self.rr(src), 'let s = "type P {\\n    i32 x;\\n    string s;\\n}\\n";')

    def test_raw_string_and_same_line(self):
        src = 'let s = r#"use A; type P { i32 x, i32 y } i32 F() { return 1; }"#;'
        self.assertEqual(self.rr(src), 'let s = r#"use A; type P { i32 x; i32 y; } i32 F() { return 1; }"#;')

    def test_escaped_quote_inside_beskid_string(self):
        src = 'let s = "type P { string s, } string G() { return \\"a, b\\"; }";'
        self.assertEqual(self.rr(src), 'let s = "type P { string s; } string G() { return \\"a, b\\"; }";')

    def test_rust_code_and_other_strings_untouched(self):
        src = 'type Alias = Vec<u8>;\nlet s = "enum E { A, B }";\n// type X { a, b }\n'
        self.assertEqual(self.rr(src), src)

    def test_keep_marker(self):
        src = '// v067_field_semicolons: keep\nlet s = "type P { i32 x, i32 y }";'
        self.assertEqual(self.rr(src), src)


class Cli(unittest.TestCase):
    def setUp(self):
        self._out = sys.stdout, sys.stderr
        sys.stdout = sys.stderr = open(os.devnull, "w")

    def tearDown(self):
        sys.stdout.close()
        sys.stdout, sys.stderr = self._out

    def test_check_dry_run_and_write(self):
        with tempfile.TemporaryDirectory() as d:
            f = os.path.join(d, "a.bd")
            skipped = os.path.join(d, "target")
            os.mkdir(skipped)
            g = os.path.join(skipped, "b.bd")
            for p in (f, g):
                _w(p, "type P { i32 x, i32 y }")
            self.assertEqual(m.main(["--check", d]), 1)
            self.assertEqual(_r(f), "type P { i32 x, i32 y }")
            self.assertEqual(m.main(["--dry-run", d]), 0)
            self.assertEqual(_r(f), "type P { i32 x, i32 y }")
            self.assertEqual(m.main([d]), 0)
            self.assertEqual(_r(f), "type P { i32 x; i32 y; }")
            self.assertEqual(_r(g), "type P { i32 x, i32 y }")
            self.assertEqual(m.main(["--check", d]), 0)


if __name__ == "__main__":
    unittest.main()

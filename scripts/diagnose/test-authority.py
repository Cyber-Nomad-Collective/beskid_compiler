#!/usr/bin/env python3
"""Unit tests for authority.py's service-table parsing: the CORELIB_SERVICES table
(find_corelib_service_entries), the CANONICAL_..._SOURCE_PATH -> real-file mapping
(canonical_path_map), and the SCHEDULER_ENTRY_HELPERS list (find_scheduler_entry_helper) --
all against synthetic source files, no real repo or builder access needed."""
import importlib.util
import pathlib
import tempfile
import unittest

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("authority", HERE / "authority.py")
authority = importlib.util.module_from_spec(spec)
spec.loader.exec_module(authority)


SOURCES_RS = '''
pub const CANONICAL_TIMER_SOURCE_PATH: &str = "src/Runtime/Timer.bd";
const CANONICAL_TIMER_SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../runtime/beskid/src/Runtime/Timer.bd"
));
'''

CORELIB_SERVICES_RS = '''
pub static CORELIB_SERVICES: &[CorelibService] = &[
    CorelibService { name: "timer_sleep_until", symbol: "__timer_sleep_until", source_path: CANONICAL_TIMER_SOURCE_PATH },
];
'''

MODULE_EMISSION_RS = '''
const SCHEDULER_ENTRY_HELPERS: &[&str] = &[
    "SchedulerContext",
    "SchedulerSetCurrentFiber",
    "ContextSwitch",
];
'''


class RepoFixture:
    def __init__(self, base: pathlib.Path):
        self.base = base
        abi_dir = base / "crates" / "beskid_abi" / "src" / "runtime_source"
        abi_dir.mkdir(parents=True)
        (abi_dir / "sources.rs").write_text(SOURCES_RS)
        (abi_dir / "corelib_services.rs").write_text(CORELIB_SERVICES_RS)
        codegen_dir = base / "crates" / "beskid_codegen" / "src"
        codegen_dir.mkdir(parents=True)
        (codegen_dir / "module_emission.rs").write_text(MODULE_EMISSION_RS)


class CanonicalPathMapTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = pathlib.Path(self.tmp.name)
        RepoFixture(self.repo)

    def tearDown(self):
        self.tmp.cleanup()

    def test_maps_source_path_const_to_real_repo_relative_path(self):
        mapping = authority.canonical_path_map(str(self.repo))
        self.assertEqual(
            mapping.get("CANONICAL_TIMER_SOURCE_PATH"),
            "runtime/beskid/src/Runtime/Timer.bd",
        )


class FindCorelibServiceEntriesTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = pathlib.Path(self.tmp.name)
        RepoFixture(self.repo)

    def tearDown(self):
        self.tmp.cleanup()

    def test_finds_entry_by_service_name(self):
        entries, services_rs = authority.find_corelib_service_entries(str(self.repo), "timer_sleep_until")
        self.assertEqual(len(entries), 1)
        self.assertEqual(entries[0]["symbol"], "__timer_sleep_until")
        self.assertEqual(entries[0]["source_path_const"], "CANONICAL_TIMER_SOURCE_PATH")
        self.assertTrue(services_rs.endswith("corelib_services.rs"))

    def test_no_entry_for_unknown_service_name(self):
        entries, _ = authority.find_corelib_service_entries(str(self.repo), "does_not_exist")
        self.assertEqual(entries, [])


class FindSchedulerEntryHelperTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = pathlib.Path(self.tmp.name)
        RepoFixture(self.repo)

    def tearDown(self):
        self.tmp.cleanup()

    def test_recognizes_listed_helper(self):
        is_helper, names = authority.find_scheduler_entry_helper(str(self.repo), "SchedulerSetCurrentFiber")
        self.assertTrue(is_helper)
        self.assertIn("SchedulerContext", names)
        self.assertIn("ContextSwitch", names)

    def test_unlisted_name_is_not_a_helper(self):
        is_helper, _ = authority.find_scheduler_entry_helper(str(self.repo), "NotAHelper")
        self.assertFalse(is_helper)


if __name__ == "__main__":
    unittest.main()

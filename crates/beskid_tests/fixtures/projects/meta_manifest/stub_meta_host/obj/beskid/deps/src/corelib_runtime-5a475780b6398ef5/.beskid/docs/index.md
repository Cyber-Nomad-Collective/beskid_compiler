# API reference

## Structure

- `Collections`
  - `Array`
    - `Advance`
      - `it`
        - `Collections::Array::Advance::it` (`parameter`)
      - `Collections::Array::Advance` (`function`)
    - `ArrayIter`
      - `index`
        - `Collections::Array::ArrayIter::index` (`field`)
      - `length`
        - `Collections::Array::ArrayIter::length` (`field`)
      - `Collections::Array::ArrayIter` (`type`)
    - `HasNext`
      - `it`
        - `Collections::Array::HasNext::it` (`parameter`)
      - `Collections::Array::HasNext` (`function`)
    - `Index`
      - `it`
        - `Collections::Array::Index::it` (`parameter`)
      - `Collections::Array::Index` (`function`)
    - `IsEmpty`
      - `values`
        - `Collections::Array::IsEmpty::values` (`parameter`)
      - `Collections::Array::IsEmpty` (`function`)
    - `Iterate`
      - `values`
        - `Collections::Array::Iterate::values` (`parameter`)
      - `Collections::Array::Iterate` (`function`)
    - `Len`
      - `values`
        - `Collections::Array::Len::values` (`parameter`)
      - `Collections::Array::Len` (`function`)
    - `Collections::Array` (`module`)
  - `List`
    - `Count`
      - `list`
        - `Collections::List::Count::list` (`parameter`)
      - `Collections::List::Count` (`function`)
    - `Get`
      - `index`
        - `Collections::List::Get::index` (`parameter`)
      - `list`
        - `Collections::List::Get::list` (`parameter`)
      - `Collections::List::Get` (`function`)
    - `IsEmpty`
      - `list`
        - `Collections::List::IsEmpty::list` (`parameter`)
      - `Collections::List::IsEmpty` (`function`)
    - `List`
      - `count`
        - `Collections::List::List::count` (`field`)
      - `Collections::List::List` (`type`)
    - `New`
      - `Collections::List::New` (`function`)
    - `Pop`
      - `list`
        - `Collections::List::Pop::list` (`parameter`)
      - `Collections::List::Pop` (`function`)
    - `Push`
      - `list`
        - `Collections::List::Push::list` (`parameter`)
      - `value`
        - `Collections::List::Push::value` (`parameter`)
      - `Collections::List::Push` (`function`)
    - `Collections::List` (`module`)
  - `Map`
    - `ContainsKey`
      - `key`
        - `Collections::Map::ContainsKey::key` (`parameter`)
      - `map`
        - `Collections::Map::ContainsKey::map` (`parameter`)
      - `Collections::Map::ContainsKey` (`function`)
    - `Count`
      - `map`
        - `Collections::Map::Count::map` (`parameter`)
      - `Collections::Map::Count` (`function`)
    - `Get`
      - `key`
        - `Collections::Map::Get::key` (`parameter`)
      - `map`
        - `Collections::Map::Get::map` (`parameter`)
      - `Collections::Map::Get` (`function`)
    - `Insert`
      - `key`
        - `Collections::Map::Insert::key` (`parameter`)
      - `map`
        - `Collections::Map::Insert::map` (`parameter`)
      - `value`
        - `Collections::Map::Insert::value` (`parameter`)
      - `Collections::Map::Insert` (`function`)
    - `IsEmpty`
      - `map`
        - `Collections::Map::IsEmpty::map` (`parameter`)
      - `Collections::Map::IsEmpty` (`function`)
    - `Map`
      - `count`
        - `Collections::Map::Map::count` (`field`)
      - `Collections::Map::Map` (`type`)
    - `MapEntry`
      - `key`
        - `Collections::Map::MapEntry::key` (`field`)
      - `value`
        - `Collections::Map::MapEntry::value` (`field`)
      - `Collections::Map::MapEntry` (`type`)
    - `New`
      - `Collections::Map::New` (`function`)
    - `Remove`
      - `key`
        - `Collections::Map::Remove::key` (`parameter`)
      - `map`
        - `Collections::Map::Remove::map` (`parameter`)
      - `Collections::Map::Remove` (`function`)
    - `Collections::Map` (`module`)
  - `Queue`
    - `Count`
      - `queue`
        - `Collections::Queue::Count::queue` (`parameter`)
      - `Collections::Queue::Count` (`function`)
    - `Dequeue`
      - `queue`
        - `Collections::Queue::Dequeue::queue` (`parameter`)
      - `Collections::Queue::Dequeue` (`function`)
    - `Enqueue`
      - `queue`
        - `Collections::Queue::Enqueue::queue` (`parameter`)
      - `value`
        - `Collections::Queue::Enqueue::value` (`parameter`)
      - `Collections::Queue::Enqueue` (`function`)
    - `IsEmpty`
      - `queue`
        - `Collections::Queue::IsEmpty::queue` (`parameter`)
      - `Collections::Queue::IsEmpty` (`function`)
    - `New`
      - `Collections::Queue::New` (`function`)
    - `Peek`
      - `queue`
        - `Collections::Queue::Peek::queue` (`parameter`)
      - `Collections::Queue::Peek` (`function`)
    - `Queue`
      - `count`
        - `Collections::Queue::Queue::count` (`field`)
      - `Collections::Queue::Queue` (`type`)
    - `Collections::Queue` (`module`)
  - `Set`
    - `Add`
      - `set`
        - `Collections::Set::Add::set` (`parameter`)
      - `value`
        - `Collections::Set::Add::value` (`parameter`)
      - `Collections::Set::Add` (`function`)
    - `Contains`
      - `set`
        - `Collections::Set::Contains::set` (`parameter`)
      - `value`
        - `Collections::Set::Contains::value` (`parameter`)
      - `Collections::Set::Contains` (`function`)
    - `Count`
      - `set`
        - `Collections::Set::Count::set` (`parameter`)
      - `Collections::Set::Count` (`function`)
    - `IsEmpty`
      - `set`
        - `Collections::Set::IsEmpty::set` (`parameter`)
      - `Collections::Set::IsEmpty` (`function`)
    - `New`
      - `Collections::Set::New` (`function`)
    - `Remove`
      - `set`
        - `Collections::Set::Remove::set` (`parameter`)
      - `value`
        - `Collections::Set::Remove::value` (`parameter`)
      - `Collections::Set::Remove` (`function`)
    - `Set`
      - `count`
        - `Collections::Set::Set::count` (`field`)
      - `Collections::Set::Set` (`type`)
    - `Collections::Set` (`module`)
  - `Stack`
    - `Count`
      - `stack`
        - `Collections::Stack::Count::stack` (`parameter`)
      - `Collections::Stack::Count` (`function`)
    - `IsEmpty`
      - `stack`
        - `Collections::Stack::IsEmpty::stack` (`parameter`)
      - `Collections::Stack::IsEmpty` (`function`)
    - `New`
      - `Collections::Stack::New` (`function`)
    - `Peek`
      - `stack`
        - `Collections::Stack::Peek::stack` (`parameter`)
      - `Collections::Stack::Peek` (`function`)
    - `Pop`
      - `stack`
        - `Collections::Stack::Pop::stack` (`parameter`)
      - `Collections::Stack::Pop` (`function`)
    - `Push`
      - `stack`
        - `Collections::Stack::Push::stack` (`parameter`)
      - `value`
        - `Collections::Stack::Push::value` (`parameter`)
      - `Collections::Stack::Push` (`function`)
    - `Stack`
      - `count`
        - `Collections::Stack::Stack::count` (`field`)
      - `Collections::Stack::Stack` (`type`)
    - `Collections::Stack` (`module`)
- `Core`
  - `ErrorHandling`
    - `ErrorInfo`
      - `code`
        - `Core::ErrorHandling::ErrorInfo::code` (`field`)
      - `message`
        - `Core::ErrorHandling::ErrorInfo::message` (`field`)
      - `Core::ErrorHandling::ErrorInfo` (`type`)
    - `IsErrorInfoEmpty`
      - `info`
        - `Core::ErrorHandling::IsErrorInfoEmpty::info` (`parameter`)
      - `Core::ErrorHandling::IsErrorInfoEmpty` (`function`)
    - `NewErrorInfo`
      - `code`
        - `Core::ErrorHandling::NewErrorInfo::code` (`parameter`)
      - `message`
        - `Core::ErrorHandling::NewErrorInfo::message` (`parameter`)
      - `Core::ErrorHandling::NewErrorInfo` (`function`)
    - `Core::ErrorHandling` (`module`)
  - `Results`
    - `IsError`
      - `value`
        - `Core::Results::IsError::value` (`parameter`)
      - `Core::Results::IsError` (`function`)
    - `IsOk`
      - `value`
        - `Core::Results::IsOk::value` (`parameter`)
      - `Core::Results::IsOk` (`function`)
    - `Result`
      - `Error`
        - `Core::Results::Result::Error` (`enum_variant`)
      - `Ok`
        - `Core::Results::Result::Ok` (`enum_variant`)
      - `error`
        - `Core::Results::Result::error` (`field`)
      - `value`
        - `Core::Results::Result::value` (`field`)
      - `Core::Results::Result` (`enum`)
    - `Core::Results` (`module`)
  - `String`
    - `Contains`
      - `needle`
        - `Core::String::Contains::needle` (`parameter`)
      - `text`
        - `Core::String::Contains::text` (`parameter`)
      - `Core::String::Contains` (`function`)
    - `IsEmpty`
      - `text`
        - `Core::String::IsEmpty::text` (`parameter`)
      - `Core::String::IsEmpty` (`function`)
    - `Len`
      - `text`
        - `Core::String::Len::text` (`parameter`)
      - `Core::String::Len` (`function`)
    - `Core::String` (`module`)
- `Query`
  - `Contracts`
    - `HasValue`
      - `value`
        - `Query::Contracts::HasValue::value` (`parameter`)
      - `Query::Contracts::HasValue` (`function`)
    - `Option`
      - `None`
        - `Query::Contracts::Option::None` (`enum_variant`)
      - `Some`
        - `Query::Contracts::Option::Some` (`enum_variant`)
      - `value`
        - `Query::Contracts::Option::value` (`field`)
      - `Query::Contracts::Option` (`enum`)
  - `Execution`
    - `IsDeferred`
      - `state`
        - `Query::Execution::IsDeferred::state` (`parameter`)
      - `Query::Execution::IsDeferred` (`function`)
    - `MaterializeCount`
      - `state`
        - `Query::Execution::MaterializeCount::state` (`parameter`)
      - `Query::Execution::MaterializeCount` (`function`)
    - `Query::Execution` (`module`)
  - `Operators`
    - `CollectArray`
      - `state`
        - `Query::Operators::CollectArray::state` (`parameter`)
      - `Query::Operators::CollectArray` (`function`)
    - `Count`
      - `state`
        - `Query::Operators::Count::state` (`parameter`)
      - `Query::Operators::Count` (`function`)
    - `First`
      - `state`
        - `Query::Operators::First::state` (`parameter`)
      - `Query::Operators::First` (`function`)
    - `QueryState`
      - `count`
        - `Query::Operators::QueryState::count` (`field`)
      - `first`
        - `Query::Operators::QueryState::first` (`field`)
      - `Query::Operators::QueryState` (`type`)
    - `Select`
      - `sample`
        - `Query::Operators::Select::sample` (`parameter`)
      - `state`
        - `Query::Operators::Select::state` (`parameter`)
      - `Query::Operators::Select` (`function`)
    - `Skip`
      - `count`
        - `Query::Operators::Skip::count` (`parameter`)
      - `state`
        - `Query::Operators::Skip::state` (`parameter`)
      - `Query::Operators::Skip` (`function`)
    - `Take`
      - `count`
        - `Query::Operators::Take::count` (`parameter`)
      - `state`
        - `Query::Operators::Take::state` (`parameter`)
      - `Query::Operators::Take` (`function`)
    - `Where`
      - `predicate`
        - `Query::Operators::Where::predicate` (`parameter`)
      - `state`
        - `Query::Operators::Where::state` (`parameter`)
      - `Query::Operators::Where` (`function`)
    - `Query::Operators` (`module`)
- `System`
  - `Environment`
    - `CurrentDirectory`
      - `System::Environment::CurrentDirectory` (`function`)
    - `EnvironmentError`
      - `InvalidName`
        - `System::Environment::EnvironmentError::InvalidName` (`enum_variant`)
      - `NotFound`
        - `System::Environment::EnvironmentError::NotFound` (`enum_variant`)
      - `UnsupportedMutation`
        - `System::Environment::EnvironmentError::UnsupportedMutation` (`enum_variant`)
      - `name`
        - `System::Environment::EnvironmentError::name` (`field`)
        - `System::Environment::EnvironmentError::name` (`field`)
      - `System::Environment::EnvironmentError` (`enum`)
    - `Get`
      - `name`
        - `System::Environment::Get::name` (`parameter`)
      - `System::Environment::Get` (`function`)
    - `GetVariable`
      - `name`
        - `System::Environment::GetVariable::name` (`parameter`)
      - `System::Environment::GetVariable` (`function`)
    - `Set`
      - `name`
        - `System::Environment::Set::name` (`parameter`)
      - `value`
        - `System::Environment::Set::value` (`parameter`)
      - `System::Environment::Set` (`function`)
    - `TryGet`
      - `name`
        - `System::Environment::TryGet::name` (`parameter`)
      - `System::Environment::TryGet` (`function`)
    - `System::Environment` (`module`)
  - `Error`
    - `Write`
      - `text`
        - `System::Error::Write::text` (`parameter`)
      - `System::Error::Write` (`function`)
    - `WriteLine`
      - `text`
        - `System::Error::WriteLine::text` (`parameter`)
      - `System::Error::WriteLine` (`function`)
    - `System::Error` (`module`)
  - `FS`
    - `CreateDirectory`
      - `path`
        - `System::FS::CreateDirectory::path` (`parameter`)
      - `System::FS::CreateDirectory` (`function`)
    - `Delete`
      - `path`
        - `System::FS::Delete::path` (`parameter`)
      - `System::FS::Delete` (`function`)
    - `Exists`
      - `path`
        - `System::FS::Exists::path` (`parameter`)
      - `System::FS::Exists` (`function`)
    - `FsError`
      - `AlreadyExists`
        - `System::FS::FsError::AlreadyExists` (`enum_variant`)
      - `InvalidPath`
        - `System::FS::FsError::InvalidPath` (`enum_variant`)
      - `NotFound`
        - `System::FS::FsError::NotFound` (`enum_variant`)
      - `PermissionDenied`
        - `System::FS::FsError::PermissionDenied` (`enum_variant`)
      - `Unknown`
        - `System::FS::FsError::Unknown` (`enum_variant`)
      - `message`
        - `System::FS::FsError::message` (`field`)
      - `path`
        - `System::FS::FsError::path` (`field`)
        - `System::FS::FsError::path` (`field`)
        - `System::FS::FsError::path` (`field`)
        - `System::FS::FsError::path` (`field`)
      - `System::FS::FsError` (`enum`)
    - `ReadAllText`
      - `path`
        - `System::FS::ReadAllText::path` (`parameter`)
      - `System::FS::ReadAllText` (`function`)
    - `WriteAllText`
      - `path`
        - `System::FS::WriteAllText::path` (`parameter`)
      - `text`
        - `System::FS::WriteAllText::text` (`parameter`)
      - `System::FS::WriteAllText` (`function`)
    - `System::FS` (`module`)
  - `Input`
    - `Read`
      - `System::Input::Read` (`function`)
    - `ReadByte`
      - `System::Input::ReadByte` (`function`)
    - `ReadLine`
      - `System::Input::ReadLine` (`function`)
    - `System::Input` (`module`)
  - `Output`
    - `Write`
      - `text`
        - `System::Output::Write::text` (`parameter`)
      - `System::Output::Write` (`function`)
    - `WriteLine`
      - `text`
        - `System::Output::WriteLine::text` (`parameter`)
      - `System::Output::WriteLine` (`function`)
    - `System::Output` (`module`)
  - `Path`
    - `Combine`
      - `left`
        - `System::Path::Combine::left` (`parameter`)
      - `right`
        - `System::Path::Combine::right` (`parameter`)
      - `System::Path::Combine` (`function`)
    - `Extension`
      - `path`
        - `System::Path::Extension::path` (`parameter`)
      - `System::Path::Extension` (`function`)
    - `FileName`
      - `path`
        - `System::Path::FileName::path` (`parameter`)
      - `System::Path::FileName` (`function`)
    - `IsAbsolute`
      - `path`
        - `System::Path::IsAbsolute::path` (`parameter`)
      - `System::Path::IsAbsolute` (`function`)
    - `IsEmpty`
      - `path`
        - `System::Path::IsEmpty::path` (`parameter`)
      - `System::Path::IsEmpty` (`function`)
    - `Separator`
      - `System::Path::Separator` (`function`)
    - `System::Path` (`module`)
  - `Process`
    - `Exit`
      - `code`
        - `System::Process::Exit::code` (`parameter`)
      - `System::Process::Exit` (`function`)
    - `ExitCode`
      - `System::Process::ExitCode` (`function`)
    - `Id`
      - `System::Process::Id` (`function`)
    - `ProcessError`
      - `InvalidCommand`
        - `System::Process::ProcessError::InvalidCommand` (`enum_variant`)
      - `SpawnFailed`
        - `System::Process::ProcessError::SpawnFailed` (`enum_variant`)
      - `command`
        - `System::Process::ProcessError::command` (`field`)
        - `System::Process::ProcessError::command` (`field`)
      - `System::Process::ProcessError` (`enum`)
    - `Run`
      - `command`
        - `System::Process::Run::command` (`parameter`)
      - `System::Process::Run` (`function`)
    - `System::Process` (`module`)
  - `Syscall`
    - `DefaultReadLimit`
      - `System::Syscall::DefaultReadLimit` (`function`)
    - `Descriptor`
      - `Descriptor`
        - `Raw`
          - `System::Syscall::Descriptor::Descriptor::Raw` (`enum_variant`)
        - `Standard`
          - `System::Syscall::Descriptor::Descriptor::Standard` (`enum_variant`)
        - `fd`
          - `System::Syscall::Descriptor::Descriptor::fd` (`field`)
        - `stream`
          - `System::Syscall::Descriptor::Descriptor::stream` (`field`)
        - `System::Syscall::Descriptor::Descriptor` (`enum`)
      - `System::Syscall::Descriptor` (`module`)
    - `Read`
      - `fd`
        - `System::Syscall::Read::fd` (`parameter`)
      - `maxBytes`
        - `System::Syscall::Read::maxBytes` (`parameter`)
      - `System::Syscall::Read` (`function`)
    - `ReadLimit`
      - `ReadLimit`
        - `Default`
          - `System::Syscall::ReadLimit::ReadLimit::Default` (`enum_variant`)
        - `UpTo`
          - `System::Syscall::ReadLimit::ReadLimit::UpTo` (`enum_variant`)
        - `maxBytes`
          - `System::Syscall::ReadLimit::ReadLimit::maxBytes` (`field`)
        - `System::Syscall::ReadLimit::ReadLimit` (`enum`)
      - `System::Syscall::ReadLimit` (`module`)
    - `ReadRequest`
      - `ReadRequest`
        - `descriptor`
          - `System::Syscall::ReadRequest::ReadRequest::descriptor` (`field`)
        - `limit`
          - `System::Syscall::ReadRequest::ReadRequest::limit` (`field`)
        - `System::Syscall::ReadRequest::ReadRequest` (`type`)
      - `System::Syscall::ReadRequest` (`module`)
    - `ReadWith`
      - `request`
        - `System::Syscall::ReadWith::request` (`parameter`)
      - `System::Syscall::ReadWith` (`function`)
    - `ResolveDescriptorFd`
      - `descriptor`
        - `System::Syscall::ResolveDescriptorFd::descriptor` (`parameter`)
      - `System::Syscall::ResolveDescriptorFd` (`function`)
    - `ResolveReadLimit`
      - `limit`
        - `System::Syscall::ResolveReadLimit::limit` (`parameter`)
      - `System::Syscall::ResolveReadLimit` (`function`)
    - `StandardStream`
      - `StandardStream`
        - `Stderr`
          - `System::Syscall::StandardStream::StandardStream::Stderr` (`enum_variant`)
        - `Stdin`
          - `System::Syscall::StandardStream::StandardStream::Stdin` (`enum_variant`)
        - `Stdout`
          - `System::Syscall::StandardStream::StandardStream::Stdout` (`enum_variant`)
        - `System::Syscall::StandardStream::StandardStream` (`enum`)
      - `System::Syscall::StandardStream` (`module`)
    - `StderrFd`
      - `System::Syscall::StderrFd` (`function`)
    - `StdinFd`
      - `System::Syscall::StdinFd` (`function`)
    - `StdoutFd`
      - `System::Syscall::StdoutFd` (`function`)
    - `SyscallError`
      - `SyscallError`
        - `InvalidFd`
          - `System::Syscall::SyscallError::SyscallError::InvalidFd` (`enum_variant`)
        - `InvalidReadLimit`
          - `System::Syscall::SyscallError::SyscallError::InvalidReadLimit` (`enum_variant`)
        - `IoFailure`
          - `System::Syscall::SyscallError::SyscallError::IoFailure` (`enum_variant`)
        - `UnsupportedReadFd`
          - `System::Syscall::SyscallError::SyscallError::UnsupportedReadFd` (`enum_variant`)
        - `code`
          - `System::Syscall::SyscallError::SyscallError::code` (`field`)
        - `fd`
          - `System::Syscall::SyscallError::SyscallError::fd` (`field`)
          - `System::Syscall::SyscallError::SyscallError::fd` (`field`)
        - `maxBytes`
          - `System::Syscall::SyscallError::SyscallError::maxBytes` (`field`)
        - `System::Syscall::SyscallError::SyscallError` (`enum`)
      - `System::Syscall::SyscallError` (`module`)
    - `Write`
      - `data`
        - `System::Syscall::Write::data` (`parameter`)
      - `fd`
        - `System::Syscall::Write::fd` (`parameter`)
      - `System::Syscall::Write` (`function`)
    - `WriteRequest`
      - `WriteRequest`
        - `data`
          - `System::Syscall::WriteRequest::WriteRequest::data` (`field`)
        - `descriptor`
          - `System::Syscall::WriteRequest::WriteRequest::descriptor` (`field`)
        - `System::Syscall::WriteRequest::WriteRequest` (`type`)
      - `System::Syscall::WriteRequest` (`module`)
    - `WriteWith`
      - `request`
        - `System::Syscall::WriteWith::request` (`parameter`)
      - `System::Syscall::WriteWith` (`function`)
    - `System::Syscall` (`module`)
  - `Time`
    - `Duration`
      - `milliseconds`
        - `System::Time::Duration::milliseconds` (`field`)
      - `System::Time::Duration` (`type`)
    - `FromMilliseconds`
      - `ms`
        - `System::Time::FromMilliseconds::ms` (`parameter`)
      - `System::Time::FromMilliseconds` (`function`)
    - `Instant`
      - `ticks`
        - `System::Time::Instant::ticks` (`field`)
      - `System::Time::Instant` (`type`)
    - `MonotonicNow`
      - `System::Time::MonotonicNow` (`function`)
    - `NowUtc`
      - `System::Time::NowUtc` (`function`)
    - `System::Time` (`module`)
- `Testing`
  - `Assertions`
    - `AssertContains`
      - `message`
        - `Testing::Assertions::AssertContains::message` (`parameter`)
      - `needle`
        - `Testing::Assertions::AssertContains::needle` (`parameter`)
      - `text`
        - `Testing::Assertions::AssertContains::text` (`parameter`)
      - `Testing::Assertions::AssertContains` (`function`)
    - `AssertEqualI64`
      - `actual`
        - `Testing::Assertions::AssertEqualI64::actual` (`parameter`)
      - `expected`
        - `Testing::Assertions::AssertEqualI64::expected` (`parameter`)
      - `message`
        - `Testing::Assertions::AssertEqualI64::message` (`parameter`)
      - `Testing::Assertions::AssertEqualI64` (`function`)
    - `AssertEqualString`
      - `actual`
        - `Testing::Assertions::AssertEqualString::actual` (`parameter`)
      - `expected`
        - `Testing::Assertions::AssertEqualString::expected` (`parameter`)
      - `message`
        - `Testing::Assertions::AssertEqualString::message` (`parameter`)
      - `Testing::Assertions::AssertEqualString` (`function`)
    - `AssertFalse`
      - `condition`
        - `Testing::Assertions::AssertFalse::condition` (`parameter`)
      - `message`
        - `Testing::Assertions::AssertFalse::message` (`parameter`)
      - `Testing::Assertions::AssertFalse` (`function`)
    - `AssertNotEqualI64`
      - `left`
        - `Testing::Assertions::AssertNotEqualI64::left` (`parameter`)
      - `message`
        - `Testing::Assertions::AssertNotEqualI64::message` (`parameter`)
      - `right`
        - `Testing::Assertions::AssertNotEqualI64::right` (`parameter`)
      - `Testing::Assertions::AssertNotEqualI64` (`function`)
    - `AssertTrue`
      - `condition`
        - `Testing::Assertions::AssertTrue::condition` (`parameter`)
      - `message`
        - `Testing::Assertions::AssertTrue::message` (`parameter`)
      - `Testing::Assertions::AssertTrue` (`function`)
    - `Fail`
      - `message`
        - `Testing::Assertions::Fail::message` (`parameter`)
      - `Testing::Assertions::Fail` (`function`)
    - `trigger_failure`
      - `message`
        - `Testing::Assertions::trigger_failure::message` (`parameter`)
      - `Testing::Assertions::trigger_failure` (`function`)
    - `Testing::Assertions` (`module`)
  - `Contracts`
    - `AssertionMessageBuilder`
      - `Build`
        - `Testing::Contracts::AssertionMessageBuilder::Build` (`contract_method`)
      - `Testing::Contracts::AssertionMessageBuilder` (`contract`)
    - `AssertionPredicate`
      - `Check`
        - `Testing::Contracts::AssertionPredicate::Check` (`contract_method`)
      - `Testing::Contracts::AssertionPredicate` (`contract`)
    - `Testing::Contracts` (`module`)
- `__alloc`
  - `__alloc` (`function`)
- `__array_len`
  - `__array_len` (`function`)
- `__array_new`
  - `__array_new` (`function`)
- `__channel_close`
  - `__channel_close` (`function`)
- `__channel_create`
  - `__channel_create` (`function`)
- `__channel_receive`
  - `__channel_receive` (`function`)
- `__channel_receive_value`
  - `__channel_receive_value` (`function`)
- `__channel_send`
  - `__channel_send` (`function`)
- `__channel_try_receive`
  - `__channel_try_receive` (`function`)
- `__channel_try_send`
  - `__channel_try_send` (`function`)
- `__fiber_cancel`
  - `__fiber_cancel` (`function`)
- `__fiber_current_id`
  - `__fiber_current_id` (`function`)
- `__fiber_detach`
  - `__fiber_detach` (`function`)
- `__fiber_join`
  - `__fiber_join` (`function`)
- `__fiber_join_value`
  - `__fiber_join_value` (`function`)
- `__fiber_now_millis`
  - `__fiber_now_millis` (`function`)
- `__fiber_processor_count`
  - `__fiber_processor_count` (`function`)
- `__fiber_spawn`
  - `__fiber_spawn` (`function`)
- `__fiber_spawn_with_cancel_slot`
  - `__fiber_spawn_with_cancel_slot` (`function`)
- `__fiber_yield`
  - `__fiber_yield` (`function`)
- `__gc_register_root`
  - `__gc_register_root` (`function`)
- `__gc_root_handle`
  - `__gc_root_handle` (`function`)
- `__gc_unregister_root`
  - `__gc_unregister_root` (`function`)
- `__gc_unroot_handle`
  - `__gc_unroot_handle` (`function`)
- `__gc_write_barrier`
  - `__gc_write_barrier` (`function`)
- `__hub_create`
  - `__hub_create` (`function`)
- `__hub_register`
  - `__hub_register` (`function`)
- `__hub_unregister`
  - `__hub_unregister` (`function`)
- `__hub_wait_receive`
  - `__hub_wait_receive` (`function`)
- `__hub_wait_receive_index`
  - `__hub_wait_receive_index` (`function`)
- `__hub_wait_receive_value`
  - `__hub_wait_receive_value` (`function`)
- `__interop_dispatch_ptr`
  - `__interop_dispatch_ptr` (`function`)
- `__interop_dispatch_unit`
  - `__interop_dispatch_unit` (`function`)
- `__interop_dispatch_usize`
  - `__interop_dispatch_usize` (`function`)
- `__mutex_create`
  - `__mutex_create` (`function`)
- `__mutex_lock`
  - `__mutex_lock` (`function`)
- `__mutex_try_lock`
  - `__mutex_try_lock` (`function`)
- `__mutex_unlock`
  - `__mutex_unlock` (`function`)
- `__panic_str`
  - `__panic_str` (`function`)
- `__str_len`
  - `__str_len` (`function`)
- `__str_new`
  - `__str_new` (`function`)
- `__syscall_read`
  - `__syscall_read` (`function`)
- `__syscall_write`
  - `__syscall_write` (`function`)
- `__test_bytes_len`
  - `__test_bytes_len` (`function`)
- `__test_bytes_ptr`
  - `__test_bytes_ptr` (`function`)
- `__wait_group_add`
  - `__wait_group_add` (`function`)
- `__wait_group_create`
  - `__wait_group_create` (`function`)
- `__wait_group_done`
  - `__wait_group_done` (`function`)
- `__wait_group_wait`
  - `__wait_group_wait` (`function`)

## Items

### `Collections::Array` (`module`)

Re-exports growable array surface.

---

### `Collections::Array::Advance` (`function`)

Advance an iterator one element.
@tier(standard)

**Type parameter `T`**
Element type of the slice (the iterator carries no payload in v1).


**Parameter `it`**
Iterator state.


**Returns**

Iterator with `index` incremented by one.


---

### `Collections::Array::Advance::it` (`parameter`)

*No documentation provided.*

---

### `Collections::Array::ArrayIter` (`type`)

Fixed-size array helper types and iteration primitives.
Slice-like `T[]` values use the runtime `BeskidArray` layout; length is read via `__array_len`.
@tier(standard)

**Type parameter `T`**
Element type for iterators.


---

### `Collections::Array::ArrayIter::index` (`field`)

Current traversal position.

---

### `Collections::Array::ArrayIter::length` (`field`)

Total number of readable elements.

---

### `Collections::Array::HasNext` (`function`)

Returns true when an iterator has remaining elements.
@tier(standard)

**Type parameter `T`**
Element type of the slice.


**Parameter `it`**
Iterator state.


**Returns**

`true` while `index < length`.


---

### `Collections::Array::HasNext::it` (`parameter`)

*No documentation provided.*

---

### `Collections::Array::Index` (`function`)

Returns the iterator's current zero-based index.
@tier(standard)

**Type parameter `T`**
Element type of the slice.


**Parameter `it`**
Iterator state.


**Returns**

`it.index`.


---

### `Collections::Array::Index::it` (`parameter`)

*No documentation provided.*

---

### `Collections::Array::IsEmpty` (`function`)

Returns true when the array carries no readable elements.
@tier(standard)

**Type parameter `T`**
Element type of the slice.


**Parameter `values`**
Slice-like array handle.


**Returns**

`true` when `Len(values) == 0`.


---

### `Collections::Array::IsEmpty::values` (`parameter`)

*No documentation provided.*

---

### `Collections::Array::Iterate` (`function`)

Creates an iterator at the first element.
@tier(standard)

**Type parameter `T`**
Element type of the slice.


**Parameter `values`**
Source array.


**Returns**

Iterator positioned at index zero.


---

### `Collections::Array::Iterate::values` (`parameter`)

*No documentation provided.*

---

### `Collections::Array::Len` (`function`)

Returns array length (element count).
@tier(standard)

**Type parameter `T`**
Element type of the slice (length is independent of `T` in v1).


**Parameter `values`**
Slice-like array handle.


**Returns**

Element count from the runtime header.


---

### `Collections::Array::Len::values` (`parameter`)

*No documentation provided.*

---

### `Collections::List` (`module`)

Re-exports list surface.

---

### `Collections::List::Count` (`function`)

Returns the logical element count.
@tier(supported)

**Parameter `list`**
List handle.


**Returns**

`count` field.


---

### `Collections::List::Count::list` (`parameter`)

*No documentation provided.*

---

### `Collections::List::Get` (`function`)

Returns an indexed element or an error when storage is unavailable.
@tier(unstable)

**Parameter `list`**
List handle.


**Parameter `index`**
Zero-based index.


**Returns**

`Error` while runtime storage is not yet wired; eventually `Ok(value)` for valid indices.


---

### `Collections::List::Get::index` (`parameter`)

*No documentation provided.*

---

### `Collections::List::Get::list` (`parameter`)

*No documentation provided.*

---

### `Collections::List::IsEmpty` (`function`)

Returns true when the list carries no logical elements.
@tier(supported)

**Parameter `list`**
List handle.


**Returns**

`true` when `count == 0`.


---

### `Collections::List::IsEmpty::list` (`parameter`)

*No documentation provided.*

---

### `Collections::List::List` (`type`)

Growable list surface; v1 tracks only logical `count` because the runtime does not yet
expose backing storage. Indexed access (`Get`) is intentionally Tier 3 until `array_get` /
`array_set` runtime builtins land.
@tier(supported)

**Type parameter `T`**
Element type stored conceptually in the list.


---

### `Collections::List::List::count` (`field`)

Number of logical elements.

---

### `Collections::List::New` (`function`)

Creates an empty list.
@tier(supported)

**Returns**

Zero-length list.


---

### `Collections::List::Pop` (`function`)

Logical pop: returns a new list with `count - 1` or zero when empty.
@tier(supported)

**Parameter `list`**
Source list handle.


**Returns**

List with `count` decremented; saturates at zero.


---

### `Collections::List::Pop::list` (`parameter`)

*No documentation provided.*

---

### `Collections::List::Push` (`function`)

Logical append: returns a new list with `count + 1`; backing storage is not yet wired.
@tier(supported)

**Parameter `list`**
Source list handle.


**Parameter `value`**
Value to record (currently observed only via count).


**Returns**

List with `count` incremented.


---

### `Collections::List::Push::list` (`parameter`)

*No documentation provided.*

---

### `Collections::List::Push::value` (`parameter`)

*No documentation provided.*

---

### `Collections::Map` (`module`)

Re-exports associative map surface.

---

### `Collections::Map::ContainsKey` (`function`)

Reports whether a key is considered present.
Without entry storage, membership cannot be observed; this returns `false` in v1 and is Tier 3
until backing storage ships.
@tier(unstable)

**Parameter `map`**
Map handle.


**Parameter `key`**
Candidate key (ignored until storage exists).


**Returns**

`false` until the runtime exposes bucket storage.


---

### `Collections::Map::ContainsKey::key` (`parameter`)

*No documentation provided.*

---

### `Collections::Map::ContainsKey::map` (`parameter`)

*No documentation provided.*

---

### `Collections::Map::Count` (`function`)

Returns the logical entry count.
@tier(supported)

**Parameter `map`**
Map handle.


**Returns**

`count` field.


---

### `Collections::Map::Count::map` (`parameter`)

*No documentation provided.*

---

### `Collections::Map::Get` (`function`)

Retrieves a value by key, or returns an error when storage is unavailable.
@tier(unstable)

**Parameter `map`**
Map handle.


**Parameter `key`**
Candidate key.


**Returns**

`Error` until runtime bucket storage ships.


---

### `Collections::Map::Get::key` (`parameter`)

*No documentation provided.*

---

### `Collections::Map::Get::map` (`parameter`)

*No documentation provided.*

---

### `Collections::Map::Insert` (`function`)

Logical insert: returns a new map with `count + 1`; storage is not yet wired so the key/value
payload is not retained between calls.
@tier(supported)

**Parameter `map`**
Source map handle.


**Parameter `key`**
Entry key (observed only via count in v1).


**Parameter `value`**
Entry value (observed only via count in v1).


**Returns**

Map with `count` incremented.


---

### `Collections::Map::Insert::key` (`parameter`)

*No documentation provided.*

---

### `Collections::Map::Insert::map` (`parameter`)

*No documentation provided.*

---

### `Collections::Map::Insert::value` (`parameter`)

*No documentation provided.*

---

### `Collections::Map::IsEmpty` (`function`)

Returns true when the map has no entries.
@tier(supported)

**Parameter `map`**
Map handle.


**Returns**

`true` when `count == 0`.


---

### `Collections::Map::IsEmpty::map` (`parameter`)

*No documentation provided.*

---

### `Collections::Map::Map` (`type`)

Logical map handle without backing storage in v1.
@tier(supported)

---

### `Collections::Map::Map::count` (`field`)

Number of logical entries.

---

### `Collections::Map::MapEntry` (`type`)

Key/value map surface; v1 tracks only logical `count` (no buckets yet). Tier 2 until runtime
bucket storage ships; signatures are stable so authors can call the parity helpers today.
@tier(supported)

**Type parameter `TKey`**
Key type parameter.


**Type parameter `TValue`**
Value type parameter.


---

### `Collections::Map::MapEntry::key` (`field`)

Entry key component.

---

### `Collections::Map::MapEntry::value` (`field`)

Entry value component.

---

### `Collections::Map::New` (`function`)

Creates an empty map.
@tier(supported)

**Returns**

Map with `count` zero.


---

### `Collections::Map::Remove` (`function`)

Logical remove: returns a new map with `count - 1` or zero when empty.
@tier(supported)

**Parameter `map`**
Source map handle.


**Parameter `key`**
Entry key (membership is not tracked in v1).


**Returns**

Map with `count` decremented; saturates at zero.


---

### `Collections::Map::Remove::key` (`parameter`)

*No documentation provided.*

---

### `Collections::Map::Remove::map` (`parameter`)

*No documentation provided.*

---

### `Collections::Queue` (`module`)

Re-exports queue surface.

---

### `Collections::Queue::Count` (`function`)

Returns the logical element count.
@tier(supported)

**Parameter `queue`**
Queue handle.


**Returns**

`count` field.


---

### `Collections::Queue::Count::queue` (`parameter`)

*No documentation provided.*

---

### `Collections::Queue::Dequeue` (`function`)

Logical dequeue: returns a new queue with `count - 1` or zero when empty.
@tier(supported)

**Parameter `queue`**
Source queue handle.


**Returns**

Queue with `count` decremented; saturates at zero.


---

### `Collections::Queue::Dequeue::queue` (`parameter`)

*No documentation provided.*

---

### `Collections::Queue::Enqueue` (`function`)

Logical enqueue: returns a new queue with `count + 1`; storage is not yet wired so the value
payload is not retained between calls.
@tier(supported)

**Parameter `queue`**
Source queue handle.


**Parameter `value`**
Element to enqueue (observed only via count in v1).


**Returns**

Queue with `count` incremented.


---

### `Collections::Queue::Enqueue::queue` (`parameter`)

*No documentation provided.*

---

### `Collections::Queue::Enqueue::value` (`parameter`)

*No documentation provided.*

---

### `Collections::Queue::IsEmpty` (`function`)

Returns true when the queue is empty.
@tier(supported)

**Parameter `queue`**
Queue handle.


**Returns**

`true` when `count == 0`.


---

### `Collections::Queue::IsEmpty::queue` (`parameter`)

*No documentation provided.*

---

### `Collections::Queue::New` (`function`)

Creates an empty queue.
@tier(supported)

**Returns**

Queue with `count` zero.


---

### `Collections::Queue::Peek` (`function`)

Returns the head element without removing it, or an error when empty / unbacked.
Tier 3 until runtime ring-buffer storage ships; signature stays stable.
@tier(unstable)

**Parameter `queue`**
Queue handle.


**Returns**

`Error` while runtime storage is not yet wired.


---

### `Collections::Queue::Peek::queue` (`parameter`)

*No documentation provided.*

---

### `Collections::Queue::Queue` (`type`)

FIFO queue shape; v1 tracks only logical `count`. Tier 2 until runtime ring-buffer storage ships;
signatures are stable so authors can call parity helpers today.
@tier(supported)

**Type parameter `T`**
Element type parameter.


---

### `Collections::Queue::Queue::count` (`field`)

Number of logical elements.

---

### `Collections::Set` (`module`)

Re-exports set surface.

---

### `Collections::Set::Add` (`function`)

Logical add: returns a new set with `count + 1`; storage is not yet wired so the value payload
is not retained between calls.
@tier(supported)

**Parameter `set`**
Source set handle.


**Parameter `value`**
Candidate element (observed only via count in v1).


**Returns**

Set with `count` incremented.


---

### `Collections::Set::Add::set` (`parameter`)

*No documentation provided.*

---

### `Collections::Set::Add::value` (`parameter`)

*No documentation provided.*

---

### `Collections::Set::Contains` (`function`)

Returns whether a value is considered present.
Without element storage, membership cannot be determined; this returns false for all inputs in v1
and is Tier 3 until backing storage ships.
@tier(unstable)

**Parameter `set`**
Set handle (only `count` is observed).


**Parameter `value`**
Candidate element (ignored until storage exists).


**Returns**

Always `false` until a backing representation is added.


---

### `Collections::Set::Contains::set` (`parameter`)

*No documentation provided.*

---

### `Collections::Set::Contains::value` (`parameter`)

*No documentation provided.*

---

### `Collections::Set::Count` (`function`)

Returns the logical element count.
@tier(supported)

**Parameter `set`**
Set handle.


**Returns**

`count` field.


---

### `Collections::Set::Count::set` (`parameter`)

*No documentation provided.*

---

### `Collections::Set::IsEmpty` (`function`)

Returns true when the set has no elements.
@tier(supported)

**Parameter `set`**
Set handle.


**Returns**

`true` when `count == 0`.


---

### `Collections::Set::IsEmpty::set` (`parameter`)

*No documentation provided.*

---

### `Collections::Set::New` (`function`)

Creates an empty set.
@tier(supported)

**Returns**

Set with `count` zero.


---

### `Collections::Set::Remove` (`function`)

Logical remove: returns a new set with `count - 1` or zero when empty.
@tier(supported)

**Parameter `set`**
Source set handle.


**Parameter `value`**
Candidate element (membership is not tracked in v1).


**Returns**

Set with `count` decremented; saturates at zero.


---

### `Collections::Set::Remove::set` (`parameter`)

*No documentation provided.*

---

### `Collections::Set::Remove::value` (`parameter`)

*No documentation provided.*

---

### `Collections::Set::Set` (`type`)

Unordered set shape; v1 tracks only logical `count` without membership storage. Tier 2 until
runtime hashed-set storage ships; signatures are stable so authors can call parity helpers today.
@tier(supported)

**Type parameter `T`**
Element type parameter.


---

### `Collections::Set::Set::count` (`field`)

Number of logical elements.

---

### `Collections::Stack` (`module`)

Re-exports stack surface.

---

### `Collections::Stack::Count` (`function`)

Returns the logical element count.
@tier(supported)

**Parameter `stack`**
Stack handle.


**Returns**

`count` field.


---

### `Collections::Stack::Count::stack` (`parameter`)

*No documentation provided.*

---

### `Collections::Stack::IsEmpty` (`function`)

Returns true when the stack is empty.
@tier(supported)

**Parameter `stack`**
Stack handle.


**Returns**

`true` when `count == 0`.


---

### `Collections::Stack::IsEmpty::stack` (`parameter`)

*No documentation provided.*

---

### `Collections::Stack::New` (`function`)

Creates an empty stack.
@tier(supported)

**Returns**

Stack with `count` zero.


---

### `Collections::Stack::Peek` (`function`)

Returns the top element without removing it, or an error when empty / unbacked.
Tier 3 until runtime stack storage ships; signature stays stable.
@tier(unstable)

**Parameter `stack`**
Stack handle.


**Returns**

`Error` while runtime storage is not yet wired.


---

### `Collections::Stack::Peek::stack` (`parameter`)

*No documentation provided.*

---

### `Collections::Stack::Pop` (`function`)

Logical pop: returns a new stack with `count - 1` or zero when empty.
@tier(supported)

**Parameter `stack`**
Source stack handle.


**Returns**

Stack with `count` decremented; saturates at zero.


---

### `Collections::Stack::Pop::stack` (`parameter`)

*No documentation provided.*

---

### `Collections::Stack::Push` (`function`)

Logical push: returns a new stack with `count + 1`; storage is not yet wired so the value
payload is not retained between calls.
@tier(supported)

**Parameter `stack`**
Source stack handle.


**Parameter `value`**
Element to push (observed only via count in v1).


**Returns**

Stack with `count` incremented.


---

### `Collections::Stack::Push::stack` (`parameter`)

*No documentation provided.*

---

### `Collections::Stack::Push::value` (`parameter`)

*No documentation provided.*

---

### `Collections::Stack::Stack` (`type`)

LIFO stack shape; v1 tracks only logical `count`. Tier 2 until runtime stack storage ships;
signatures are stable so authors can call parity helpers today.
@tier(supported)

**Type parameter `T`**
Element type parameter.


---

### `Collections::Stack::Stack::count` (`field`)

Number of logical elements.

---

### `Core::ErrorHandling` (`module`)

Re-exports structured error metadata.

---

### `Core::ErrorHandling::ErrorInfo` (`type`)

Lightweight error metadata for user-facing messages.

---

### `Core::ErrorHandling::ErrorInfo::code` (`field`)

Machine-readable error identifier.

---

### `Core::ErrorHandling::ErrorInfo::message` (`field`)

Human-readable error description.

---

### `Core::ErrorHandling::IsErrorInfoEmpty` (`function`)

Returns true when both ErrorInfo fields are empty.

**Parameter `info`**
Metadata record to inspect.


**Returns**

`true` when both fields are empty strings.


---

### `Core::ErrorHandling::IsErrorInfoEmpty::info` (`parameter`)

*No documentation provided.*

---

### `Core::ErrorHandling::NewErrorInfo` (`function`)

Constructs an ErrorInfo from a code and message.

**Parameter `code`**
Stable machine-readable code.


**Parameter `message`**
Human-readable explanation.


**Returns**

Populated `ErrorInfo`.


---

### `Core::ErrorHandling::NewErrorInfo::code` (`parameter`)

*No documentation provided.*

---

### `Core::ErrorHandling::NewErrorInfo::message` (`parameter`)

*No documentation provided.*

---

### `Core::Results` (`module`)

Foundation package prelude: core types, collections, query helpers, and testing contracts.
Re-exports result and error carrier types.

---

### `Core::Results::IsError` (`function`)

Returns true when result carries `Error`.

**Parameter `value`**
Result to inspect.


**Returns**

`true` when `value` is `Error`.


---

### `Core::Results::IsError::value` (`parameter`)

*No documentation provided.*

---

### `Core::Results::IsOk` (`function`)

Returns true when result carries `Ok`.

**Parameter `value`**
Result to inspect.


**Returns**

`true` when `value` is `Ok`.


---

### `Core::Results::IsOk::value` (`parameter`)

*No documentation provided.*

---

### `Core::Results::Result` (`enum`)

Discriminated success or failure carrier used across corelib and user code.

**Type parameter `TValue`**
Type of the successful payload.


**Type parameter `TError`**
Type of the failure payload.


**Variant `Ok`**
Successful branch carrying `value`.


**Variant `Error`**
Failure branch carrying `error`.


---

### `Core::Results::Result::Error` (`enum_variant`)

Carries the failure payload.

---

### `Core::Results::Result::Ok` (`enum_variant`)

Carries the successful payload.

---

### `Core::Results::Result::error` (`field`)

*No documentation provided.*

---

### `Core::Results::Result::value` (`field`)

*No documentation provided.*

---

### `Core::String` (`module`)

Re-exports string helper functions.

---

### `Core::String::Contains` (`function`)

Returns true when needle is treated as present in text (prefix/suffix equality only in v1).

**Parameter `text`**
Haystack UTF-8 string.


**Parameter `needle`**
Needle UTF-8 string; empty needle is always considered found.


**Returns**

`true` when `needle` is empty, longer than `text`, or exactly equal to `text`.


---

### `Core::String::Contains::needle` (`parameter`)

*No documentation provided.*

---

### `Core::String::Contains::text` (`parameter`)

*No documentation provided.*

---

### `Core::String::IsEmpty` (`function`)

Returns true when text is empty.

**Parameter `text`**
UTF-8 string handle.


**Returns**

`true` when `Len` is zero.


---

### `Core::String::IsEmpty::text` (`parameter`)

*No documentation provided.*

---

### `Core::String::Len` (`function`)

UTF-8 string helpers backed by runtime string builtins (`__str_len`).
Returns the number of UTF-8 code units in text.

**Parameter `text`**
UTF-8 string handle.


**Returns**

Code unit count as `i64`.


---

### `Core::String::Len::text` (`parameter`)

*No documentation provided.*

---

### `Query::Contracts::HasValue` (`function`)

Returns true when the option is `Some`.

**Parameter `value`**
Option to inspect.


**Returns**

`true` when `value` is `Some`.


---

### `Query::Contracts::HasValue::value` (`parameter`)

*No documentation provided.*

---

### `Query::Contracts::Option` (`enum`)

Optional value carrier for query iterators and environment probes.

**Type parameter `T`**
Element type stored in `Some`.


**Variant `Some`**
Carries a present `value`.


**Variant `None`**
Represents absence.


---

### `Query::Contracts::Option::None` (`enum_variant`)

Absent value variant.

---

### `Query::Contracts::Option::Some` (`enum_variant`)

Present value variant.

---

### `Query::Contracts::Option::value` (`field`)

*No documentation provided.*

---

### `Query::Execution` (`module`)

Re-exports query execution helpers.

---

### `Query::Execution::IsDeferred` (`function`)

Query execution helpers for deferred state checks and materialization.
Returns true when the state reports elements but no concrete `first` value yet (lazy placeholder).

**Parameter `state`**
Query state to inspect.


**Returns**

`true` when `count > 0` and `first` is `None`.


---

### `Query::Execution::IsDeferred::state` (`parameter`)

*No documentation provided.*

---

### `Query::Execution::MaterializeCount` (`function`)

Materializes and returns element count.

**Parameter `state`**
Query state.


**Returns**

Same as `Query.Operators.Count`.


---

### `Query::Execution::MaterializeCount::state` (`parameter`)

*No documentation provided.*

---

### `Query::Operators` (`module`)

Re-exports query operator state and combinators (imports `Query.Contracts` internally).

---

### `Query::Operators::CollectArray` (`function`)

Collects state into an array-like count placeholder.

**Parameter `state`**
Query state.


**Returns**

Element count (no backing array yet).


---

### `Query::Operators::CollectArray::state` (`parameter`)

*No documentation provided.*

---

### `Query::Operators::Count` (`function`)

Returns current logical element count.

**Parameter `state`**
Query state.


**Returns**

Logical length.


---

### `Query::Operators::Count::state` (`parameter`)

*No documentation provided.*

---

### `Query::Operators::First` (`function`)

Returns the optional first element recorded on the state.

**Parameter `state`**
Query state.


**Returns**

`first` field unchanged.


---

### `Query::Operators::First::state` (`parameter`)

*No documentation provided.*

---

### `Query::Operators::QueryState` (`type`)

Query combinators and state carrier types for lightweight LINQ-style scaffolding.

**Type parameter `T`**
Element type tracked by the query state.


---

### `Query::Operators::QueryState::count` (`field`)

Number of elements represented by state.

---

### `Query::Operators::QueryState::first` (`field`)

Optional first element placeholder for materialized pipelines.

---

### `Query::Operators::Select` (`function`)

Projects state into another element type, carrying a sample value for the `first` slot.

**Parameter `state`**
Source state over `TIn`.


**Parameter `sample`**
Representative `TOut` value used for the optional first slot.


**Returns**

New state over `TOut`.


---

### `Query::Operators::Select::sample` (`parameter`)

*No documentation provided.*

---

### `Query::Operators::Select::state` (`parameter`)

*No documentation provided.*

---

### `Query::Operators::Skip` (`function`)

Removes up to count elements from state.

**Parameter `state`**
Source state.


**Parameter `count`**
Elements to drop from the logical front.


**Returns**

State after skipping.


---

### `Query::Operators::Skip::count` (`parameter`)

*No documentation provided.*

---

### `Query::Operators::Skip::state` (`parameter`)

*No documentation provided.*

---

### `Query::Operators::Take` (`function`)

Limits state to at most count elements.

**Parameter `state`**
Source state.


**Parameter `count`**
Maximum elements to retain.


**Returns**

Truncated state.


---

### `Query::Operators::Take::count` (`parameter`)

*No documentation provided.*

---

### `Query::Operators::Take::state` (`parameter`)

*No documentation provided.*

---

### `Query::Operators::Where` (`function`)

Keeps or clears state based on predicate.

**Parameter `state`**
Incoming query state.


**Parameter `predicate`**
When false, the pipeline resets to empty.


**Returns**

Filtered state.


---

### `Query::Operators::Where::predicate` (`parameter`)

*No documentation provided.*

---

### `Query::Operators::Where::state` (`parameter`)

*No documentation provided.*

---

### `System::Environment` (`module`)

Re-exports environment helpers.

---

### `System::Environment::CurrentDirectory` (`function`)

Returns the current working directory path.

**Returns**

Placeholder `"."` until host integration lands.


---

### `System::Environment::EnvironmentError` (`enum`)

Process environment variable helpers.[Query::Contracts::Option](/docs/corelib_foundation%400.1.0/api/Query%3A%3AContracts%3A%3AOption)optional lookup surface for `TryGet`.[Core::Results::Result](/docs/corelib_foundation%400.1.0/api/Core%3A%3AResults%3A%3AResult)result envelopes for strict getters/setters.

**Variant `InvalidName`**
Name rejected as empty or malformed.


**Variant `NotFound`**
Variable is not available in this host snapshot.


**Variant `UnsupportedMutation`**
Writes are not permitted on this surface.


---

### `System::Environment::EnvironmentError::InvalidName` (`enum_variant`)

Variable name is empty or invalid.

---

### `System::Environment::EnvironmentError::NotFound` (`enum_variant`)

Variable is not available.

---

### `System::Environment::EnvironmentError::UnsupportedMutation` (`enum_variant`)

Mutation is not supported.

---

### `System::Environment::EnvironmentError::name` (`field`)

*No documentation provided.*

---

### `System::Environment::EnvironmentError::name` (`field`)

*No documentation provided.*

---

### `System::Environment::Get` (`function`)

Returns a variable value or an error.

**Parameter `name`**
Environment variable name.


**Returns**

`Error` for unsupported hosts in v1.


---

### `System::Environment::Get::name` (`parameter`)

*No documentation provided.*

---

### `System::Environment::GetVariable` (`function`)

Alias for Get.

**Parameter `name`**
Environment variable name.


---

### `System::Environment::GetVariable::name` (`parameter`)

*No documentation provided.*

---

### `System::Environment::Set` (`function`)

Attempts to set a variable.

**Parameter `name`**
Environment variable name.


**Parameter `value`**
Desired value.


**Returns**

`Error` for unsupported mutation paths in v1.


---

### `System::Environment::Set::name` (`parameter`)

*No documentation provided.*

---

### `System::Environment::Set::value` (`parameter`)

*No documentation provided.*

---

### `System::Environment::TryGet` (`function`)

Returns an optional variable value.

**Parameter `name`**
Environment variable name.


**Returns**

`None` when unavailable or invalid in v1.


---

### `System::Environment::TryGet::name` (`parameter`)

*No documentation provided.*

---

### `System::Error` (`module`)

Re-exports standard error helpers.

---

### `System::Error::Write` (`function`)

Writes text to standard error without a trailing newline.

**Parameter `text`**
UTF-8 payload.


---

### `System::Error::Write::text` (`parameter`)

*No documentation provided.*

---

### `System::Error::WriteLine` (`function`)

Writes text followed by a platform newline to standard error.

**Parameter `text`**
UTF-8 payload written before the newline.


---

### `System::Error::WriteLine::text` (`parameter`)

*No documentation provided.*

---

### `System::FS` (`module`)

Re-exports filesystem helpers.

---

### `System::FS::CreateDirectory` (`function`)

Creates a directory at `path` (parents are not created in v1).
@tier(supported)

**Parameter `path`**
Target directory path.


**Returns**

`Unknown` until runtime support lands; `InvalidPath` when input is empty.


---

### `System::FS::CreateDirectory::path` (`parameter`)

*No documentation provided.*

---

### `System::FS::Delete` (`function`)

Removes a file or empty directory at a path.
@tier(supported)

**Parameter `path`**
Target filesystem path.


**Returns**

`Ok(true)` placeholder for the no-op (empty payload) case; otherwise `Unknown` until
runtime support lands.

---

### `System::FS::Delete::path` (`parameter`)

*No documentation provided.*

---

### `System::FS::Exists` (`function`)

Reports whether a path is considered to exist.
Tier 2: returns `true` for any non-empty input until the runtime exposes a stat syscall;
the contract is stable so authors can write conditional code today.
@tier(supported)

**Parameter `path`**
Candidate path string.


**Returns**

`true` when non-empty in this v1 heuristic stub; `false` for empty.


---

### `System::FS::Exists::path` (`parameter`)

*No documentation provided.*

---

### `System::FS::FsError` (`enum`)

File system helper API.
Tier 2 (Supported) — body wiring depends on the host-syscall path landing in `System.Syscall`.
Signatures are stable for v0.3 so authors can call them today; bodies return deterministic
errors until the runtime exposes filesystem syscalls beyond stdin/stdout/stderr.
@tier(supported)[Core::Results::Result](/docs/corelib_foundation%400.1.0/api/Core%3A%3AResults%3A%3AResult)envelopes for read/write helpers.

**Variant `NotFound`**
Requested path is missing in this v1 stub.


**Variant `PermissionDenied`**
Host denied access to the path.


**Variant `AlreadyExists`**
Target path already exists when an exclusive create was requested.


**Variant `InvalidPath`**
Input path is empty or otherwise rejected before reaching the host.


**Variant `Unknown`**
Catch-all for unimplemented or internal failures.


---

### `System::FS::FsError::AlreadyExists` (`enum_variant`)

Target path already exists.

---

### `System::FS::FsError::InvalidPath` (`enum_variant`)

Input path was rejected by validation before the host syscall.

---

### `System::FS::FsError::NotFound` (`enum_variant`)

Path cannot be found.

---

### `System::FS::FsError::PermissionDenied` (`enum_variant`)

Access rights are insufficient.

---

### `System::FS::FsError::Unknown` (`enum_variant`)

Uncategorized file system failure.

---

### `System::FS::FsError::message` (`field`)

*No documentation provided.*

---

### `System::FS::FsError::path` (`field`)

*No documentation provided.*

---

### `System::FS::FsError::path` (`field`)

*No documentation provided.*

---

### `System::FS::FsError::path` (`field`)

*No documentation provided.*

---

### `System::FS::FsError::path` (`field`)

*No documentation provided.*

---

### `System::FS::ReadAllText` (`function`)

Reads full text content from a path.
@tier(supported)

**Parameter `path`**
Filesystem path to read.


**Returns**

File contents or an `FsError`. v1 returns `Unknown` for non-empty paths and
`InvalidPath` for empty input.

---

### `System::FS::ReadAllText::path` (`parameter`)

*No documentation provided.*

---

### `System::FS::WriteAllText` (`function`)

Writes full text content to a path.
@tier(supported)

**Parameter `path`**
Target filesystem path.


**Parameter `text`**
Payload to persist.


**Returns**

`Ok(true)` for empty payloads on a non-empty path (no-op); otherwise `Unknown` until
runtime support lands.

---

### `System::FS::WriteAllText::path` (`parameter`)

*No documentation provided.*

---

### `System::FS::WriteAllText::text` (`parameter`)

*No documentation provided.*

---

### `System::Input` (`module`)

Re-exports standard input helpers.

---

### `System::Input::Read` (`function`)

Reads up to the default byte limit from standard input.

**Returns**

Payload string or a `SyscallError`.


---

### `System::Input::ReadByte` (`function`)

Reads a single byte from stdin (bounded read of one code unit).

---

### `System::Input::ReadLine` (`function`)

Reads a line from standard input until `\n` or EOF (excludes the newline).

**Returns**

Payload string or a `SyscallError`.


---

### `System::Output` (`module`)

Re-exports standard output helpers.

---

### `System::Output::Write` (`function`)

Writes text to standard output without a trailing newline.

**Parameter `text`**
UTF-8 payload.


**Returns**

Unit; panics when the underlying write returns an error in v0.2.


---

### `System::Output::Write::text` (`parameter`)

*No documentation provided.*

---

### `System::Output::WriteLine` (`function`)

Writes text followed by a platform newline.

**Parameter `text`**
UTF-8 payload written before the newline.


---

### `System::Output::WriteLine::text` (`parameter`)

*No documentation provided.*

---

### `System::Path` (`module`)

Re-exports path helpers.

---

### `System::Path::Combine` (`function`)

Joins two path segments with the platform separator.
@tier(supported)

**Parameter `left`**
Leading segment; may be empty to yield `right` only.


**Parameter `right`**
Trailing segment; may be empty to yield `left` only.


**Returns**

Segments joined with `/`.


---

### `System::Path::Combine::left` (`parameter`)

*No documentation provided.*

---

### `System::Path::Combine::right` (`parameter`)

*No documentation provided.*

---

### `System::Path::Extension` (`function`)

Returns the extension component.
Tier 3 until a full segmenter is wired; signature stays stable.
@tier(unstable)

**Parameter `path`**
Full or relative path input.


**Returns**

Identity placeholder until parsing is implemented.


---

### `System::Path::Extension::path` (`parameter`)

*No documentation provided.*

---

### `System::Path::FileName` (`function`)

Returns the file name component.
Tier 3 until a full segmenter is wired; signature stays stable.
@tier(unstable)

**Parameter `path`**
Full or relative path input.


**Returns**

Identity placeholder until parsing is implemented.


---

### `System::Path::FileName::path` (`parameter`)

*No documentation provided.*

---

### `System::Path::IsAbsolute` (`function`)

Returns true when the path is the POSIX root (`/`).
Full prefix detection is Tier 3 until string slicing builtins ship; this v1 helper covers the
degenerate root case so `Combine("/", x)` consumers can short-circuit safely.
@tier(unstable)

**Parameter `path`**
Candidate path string.


**Returns**

`true` when `path` equals `/` exactly; `false` for everything else.


---

### `System::Path::IsAbsolute::path` (`parameter`)

*No documentation provided.*

---

### `System::Path::IsEmpty` (`function`)

Reports whether the path is empty.
@tier(supported)

**Parameter `path`**
Candidate path string.


**Returns**

`true` when `path` is the empty string.


---

### `System::Path::IsEmpty::path` (`parameter`)

*No documentation provided.*

---

### `System::Path::Separator` (`function`)

Forward-slash separator used by the v1 stub helpers.
Tier 2 (Supported) — stable signature; the value is constant for POSIX targets.
@tier(supported)

**Returns**

Literal `"/"`.


---

### `System::Process` (`module`)

Re-exports process helpers.

---

### `System::Process::Exit` (`function`)

Terminates current process with a code.

**Parameter `code`**
Exit status; non-zero values panic in v1 until runtime-backed exits exist.


---

### `System::Process::Exit::code` (`parameter`)

*No documentation provided.*

---

### `System::Process::ExitCode` (`function`)

Returns current process exit code when available.

**Returns**

`Ok(0)` placeholder until host metadata is wired.


---

### `System::Process::Id` (`function`)

Returns current process identifier.

**Returns**

Placeholder `0` until host integration lands.


---

### `System::Process::ProcessError` (`enum`)

Process metadata and control helpers.[Core::Results::Result](/docs/corelib_foundation%400.1.0/api/Core%3A%3AResults%3A%3AResult)for exit metadata helpers.

**Variant `InvalidCommand`**
Empty or otherwise unusable command line.


**Variant `SpawnFailed`**
Host could not spawn the requested process.


---

### `System::Process::ProcessError::InvalidCommand` (`enum_variant`)

Command is empty or invalid.

---

### `System::Process::ProcessError::SpawnFailed` (`enum_variant`)

Process creation failed.

---

### `System::Process::ProcessError::command` (`field`)

*No documentation provided.*

---

### `System::Process::ProcessError::command` (`field`)

*No documentation provided.*

---

### `System::Process::Run` (`function`)

Attempts to execute a command.

**Parameter `command`**
Command line to spawn.


**Returns**

`SpawnFailed` for non-empty commands in this stub.


---

### `System::Process::Run::command` (`parameter`)

*No documentation provided.*

---

### `System::Syscall` (`module`)

Runtime package prelude: host syscall facades and system integration.
Re-exports syscall facade helpers.`System.Syscall` _(unresolved)_low-level read/write entry points.`System.Input` _(unresolved)_stdin read helpers.`System.Output` _(unresolved)_stdout write helpers.`System.Error` _(unresolved)_stderr write helpers.

---

### `System::Syscall::DefaultReadLimit` (`function`)

Returns default byte limit used by `ReadWith`.

**Returns**

Default cap passed to `__syscall_read` when `ReadLimit::Default` is selected.


---

### `System::Syscall::Descriptor` (`module`)

Re-exports typed descriptor selectors.

---

### `System::Syscall::Descriptor::Descriptor` (`enum`)

Descriptor selector for syscall wrappers.

**Variant `Standard`**
Well-known stdin/stdout/stderr mapping.


**Variant `Raw`**
Opaque numeric descriptor supplied by the host.


---

### `System::Syscall::Descriptor::Descriptor::Raw` (`enum_variant`)

Use a raw numeric descriptor.

---

### `System::Syscall::Descriptor::Descriptor::Standard` (`enum_variant`)

Use one of the runtime-defined standard streams.

---

### `System::Syscall::Descriptor::Descriptor::fd` (`field`)

*No documentation provided.*

---

### `System::Syscall::Descriptor::Descriptor::stream` (`field`)

*No documentation provided.*

---

### `System::Syscall::Read` (`function`)

Reads up to max bytes from a descriptor.

**Parameter `fd`**
Numeric host descriptor (stdin-only in v1).


**Parameter `maxBytes`**
Upper bound on returned string length.


**Returns**

Payload string or a `SyscallError`.


---

### `System::Syscall::Read::fd` (`parameter`)

*No documentation provided.*

---

### `System::Syscall::Read::maxBytes` (`parameter`)

*No documentation provided.*

---

### `System::Syscall::ReadLimit` (`module`)

Re-exports typed read-limit selectors.

---

### `System::Syscall::ReadLimit::ReadLimit` (`enum`)

Read-limit selector for syscall wrappers.

**Variant `UpTo`**
Explicit positive cap in bytes.


**Variant `Default`**
Delegates to `System.Syscall.DefaultReadLimit`.


---

### `System::Syscall::ReadLimit::ReadLimit::Default` (`enum_variant`)

Use corelib default read size.

---

### `System::Syscall::ReadLimit::ReadLimit::UpTo` (`enum_variant`)

Use explicit byte count.

---

### `System::Syscall::ReadLimit::ReadLimit::maxBytes` (`field`)

*No documentation provided.*

---

### `System::Syscall::ReadRequest` (`module`)

Re-exports read request payload type.

---

### `System::Syscall::ReadRequest::ReadRequest` (`type`)

Typed read request payload for syscall-based reads.

**Type parameter `descriptor`**
Logical source for the read.


**Type parameter `limit`**
Byte cap selection forwarded to `Read`.


---

### `System::Syscall::ReadRequest::ReadRequest::descriptor` (`field`)

Descriptor selector for read source.

---

### `System::Syscall::ReadRequest::ReadRequest::limit` (`field`)

Read-size option for this call.

---

### `System::Syscall::ReadWith` (`function`)

Reads data based on typed descriptor and read-limit options.

**Parameter `request`**
Descriptor, limit, and implicit stdin routing rules for v1.


**Returns**

Payload string or a `SyscallError`.


---

### `System::Syscall::ReadWith::request` (`parameter`)

*No documentation provided.*

---

### `System::Syscall::ResolveDescriptorFd` (`function`)

Resolves a typed descriptor into a numeric fd.

**Parameter `descriptor`**
Logical stream or raw fd selector.


**Returns**

Host numeric descriptor suitable for `Read` / `Write`.


---

### `System::Syscall::ResolveDescriptorFd::descriptor` (`parameter`)

*No documentation provided.*

---

### `System::Syscall::ResolveReadLimit` (`function`)

Resolves read-limit options to a concrete byte size.

**Parameter `limit`**
Explicit cap or default sentinel.


**Returns**

Positive byte count forwarded to `Read`.


---

### `System::Syscall::ResolveReadLimit::limit` (`parameter`)

*No documentation provided.*

---

### `System::Syscall::StandardStream` (`module`)

Re-exports stream descriptor variants.

---

### `System::Syscall::StandardStream::StandardStream` (`enum`)

Standard stream handles normalized by runtime conventions.

**Variant `Stdin`**
Standard input.


**Variant `Stdout`**
Standard output.


**Variant `Stderr`**
Standard error.


---

### `System::Syscall::StandardStream::StandardStream::Stderr` (`enum_variant`)

Standard error stream.

---

### `System::Syscall::StandardStream::StandardStream::Stdin` (`enum_variant`)

Standard input stream.

---

### `System::Syscall::StandardStream::StandardStream::Stdout` (`enum_variant`)

Standard output stream.

---

### `System::Syscall::StderrFd` (`function`)

Returns descriptor for standard error.

**Returns**

POSIX-style stderr file descriptor constant (`2`).


---

### `System::Syscall::StdinFd` (`function`)

Returns descriptor for standard input.

**Returns**

POSIX-style stdin file descriptor constant (`0`).


---

### `System::Syscall::StdoutFd` (`function`)

Returns descriptor for standard output.

**Returns**

POSIX-style stdout file descriptor constant (`1`).


---

### `System::Syscall::SyscallError` (`module`)

Re-exports syscall error variants.

---

### `System::Syscall::SyscallError::SyscallError` (`enum`)

Error variants for common syscall wrapper operations.

**Variant `InvalidFd`**
Negative or otherwise unusable descriptor.


**Variant `UnsupportedReadFd`**
Read attempted on a non-stdin descriptor in v1.


**Variant `InvalidReadLimit`**
Non-positive read cap.


**Variant `IoFailure`**
Negative syscall return code surfaced as error payload.


---

### `System::Syscall::SyscallError::SyscallError::InvalidFd` (`enum_variant`)

Descriptor is invalid.

---

### `System::Syscall::SyscallError::SyscallError::InvalidReadLimit` (`enum_variant`)

Requested byte limit is invalid.

---

### `System::Syscall::SyscallError::SyscallError::IoFailure` (`enum_variant`)

Runtime syscall reported an I/O failure code.

---

### `System::Syscall::SyscallError::SyscallError::UnsupportedReadFd` (`enum_variant`)

Reads are unsupported on descriptor.

---

### `System::Syscall::SyscallError::SyscallError::code` (`field`)

*No documentation provided.*

---

### `System::Syscall::SyscallError::SyscallError::fd` (`field`)

*No documentation provided.*

---

### `System::Syscall::SyscallError::SyscallError::fd` (`field`)

*No documentation provided.*

---

### `System::Syscall::SyscallError::SyscallError::maxBytes` (`field`)

*No documentation provided.*

---

### `System::Syscall::Write` (`function`)

Writes data to a descriptor.

**Parameter `fd`**
Numeric host descriptor.


**Parameter `data`**
UTF-8 payload.


**Returns**

Written byte count or a `SyscallError` when validation or syscall fails.


---

### `System::Syscall::Write::data` (`parameter`)

*No documentation provided.*

---

### `System::Syscall::Write::fd` (`parameter`)

*No documentation provided.*

---

### `System::Syscall::WriteRequest` (`module`)

Re-exports write request payload type.

---

### `System::Syscall::WriteRequest::WriteRequest` (`type`)

Typed write request payload for syscall-based writes.

**Type parameter `descriptor`**
Logical sink for the write.


**Type parameter `data`**
UTF-8 payload handed to `__syscall_write`.


---

### `System::Syscall::WriteRequest::WriteRequest::data` (`field`)

UTF-8 data payload to write.

---

### `System::Syscall::WriteRequest::WriteRequest::descriptor` (`field`)

Descriptor selector for write target.

---

### `System::Syscall::WriteWith` (`function`)

Writes data based on a typed descriptor request.

**Parameter `request`**
Descriptor plus payload bundle.


**Returns**

Written byte count or a `SyscallError`.


---

### `System::Syscall::WriteWith::request` (`parameter`)

*No documentation provided.*

---

### `System::Time` (`module`)

Re-exports time helpers.

---

### `System::Time::Duration` (`type`)

*No documentation provided.*

---

### `System::Time::Duration::milliseconds` (`field`)

Duration length in milliseconds.

---

### `System::Time::FromMilliseconds` (`function`)

Constructs duration from millisecond value.

**Parameter `ms`**
Signed span in milliseconds.


**Returns**

`Duration` carrying the same millisecond count.


---

### `System::Time::FromMilliseconds::ms` (`parameter`)

*No documentation provided.*

---

### `System::Time::Instant` (`type`)

Time types for instants and durations.

**Type parameter `ticks`**
Monotonic or wall-clock tick payload for `Instant` (host-defined scale).


**Type parameter `milliseconds`**
Signed millisecond span for `Duration`.


---

### `System::Time::Instant::ticks` (`field`)

Tick value for the represented instant.

---

### `System::Time::MonotonicNow` (`function`)

Returns current monotonic instant.

**Returns**

Placeholder instant with `ticks = 0` until host clocks are wired.


---

### `System::Time::NowUtc` (`function`)

Returns current UTC instant.

**Returns**

Placeholder instant with `ticks = 0` until host clocks are wired.


---

### `Testing::Assertions` (`module`)

Re-exports assertion helper functions.

---

### `Testing::Assertions::AssertContains` (`function`)

Fails when needle is not found in text.

**Parameter `text`**
Haystack UTF-8 string.


**Parameter `needle`**
Needle UTF-8 string.


**Parameter `message`**
Diagnostic when `Contains` fails.


---

### `Testing::Assertions::AssertContains::message` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertContains::needle` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertContains::text` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertEqualI64` (`function`)

Fails when expected and actual i64 values differ.

**Parameter `expected`**
Expected integral value.


**Parameter `actual`**
Actual integral value.


**Parameter `message`**
Diagnostic when values differ.


---

### `Testing::Assertions::AssertEqualI64::actual` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertEqualI64::expected` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertEqualI64::message` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertEqualString` (`function`)

Fails when expected and actual strings differ.

**Parameter `expected`**
Expected UTF-8 text.


**Parameter `actual`**
Actual UTF-8 text.


**Parameter `message`**
Diagnostic when values differ.


---

### `Testing::Assertions::AssertEqualString::actual` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertEqualString::expected` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertEqualString::message` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertFalse` (`function`)

Fails when condition is true.

**Parameter `condition`**
Predicate that must be false.


**Parameter `message`**
Diagnostic when the assertion fails.


---

### `Testing::Assertions::AssertFalse::condition` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertFalse::message` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertNotEqualI64` (`function`)

Fails when both i64 values are equal.

**Parameter `left`**
First operand.


**Parameter `right`**
Second operand.


**Parameter `message`**
Diagnostic when values match unexpectedly.


---

### `Testing::Assertions::AssertNotEqualI64::left` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertNotEqualI64::message` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertNotEqualI64::right` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertTrue` (`function`)

Fails when condition is not true.

**Parameter `condition`**
Predicate that must hold.


**Parameter `message`**
Diagnostic when the assertion fails.


---

### `Testing::Assertions::AssertTrue::condition` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::AssertTrue::message` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::Fail` (`function`)

Unconditionally fails the current test.

**Parameter `message`**
Failure reason surfaced to the user.


---

### `Testing::Assertions::Fail::message` (`parameter`)

*No documentation provided.*

---

### `Testing::Assertions::trigger_failure` (`function`)

Emits a message and forces a runtime crash.

**Parameter `message`**
Text printed before the intentional fault.


---

### `Testing::Assertions::trigger_failure::message` (`parameter`)

*No documentation provided.*

---

### `Testing::Contracts` (`module`)

Re-exports assertion-related contracts.

---

### `Testing::Contracts::AssertionMessageBuilder` (`contract`)

Contract for deferred message creation.

---

### `Testing::Contracts::AssertionMessageBuilder::Build` (`contract_method`)

Builds diagnostic assertion text.

---

### `Testing::Contracts::AssertionPredicate` (`contract`)

Contracts for assertion helpers used by tests.
Contract for deferred assertion predicates.

---

### `Testing::Contracts::AssertionPredicate::Check` (`contract_method`)

Evaluates predicate pass/fail status.

---

### `__alloc` (`function`)

*No documentation provided.*

---

### `__array_len` (`function`)

*No documentation provided.*

---

### `__array_new` (`function`)

*No documentation provided.*

---

### `__channel_close` (`function`)

*No documentation provided.*

---

### `__channel_create` (`function`)

*No documentation provided.*

---

### `__channel_receive` (`function`)

*No documentation provided.*

---

### `__channel_receive_value` (`function`)

*No documentation provided.*

---

### `__channel_send` (`function`)

*No documentation provided.*

---

### `__channel_try_receive` (`function`)

*No documentation provided.*

---

### `__channel_try_send` (`function`)

*No documentation provided.*

---

### `__fiber_cancel` (`function`)

*No documentation provided.*

---

### `__fiber_current_id` (`function`)

*No documentation provided.*

---

### `__fiber_detach` (`function`)

*No documentation provided.*

---

### `__fiber_join` (`function`)

*No documentation provided.*

---

### `__fiber_join_value` (`function`)

*No documentation provided.*

---

### `__fiber_now_millis` (`function`)

*No documentation provided.*

---

### `__fiber_processor_count` (`function`)

*No documentation provided.*

---

### `__fiber_spawn` (`function`)

*No documentation provided.*

---

### `__fiber_spawn_with_cancel_slot` (`function`)

*No documentation provided.*

---

### `__fiber_yield` (`function`)

*No documentation provided.*

---

### `__gc_register_root` (`function`)

*No documentation provided.*

---

### `__gc_root_handle` (`function`)

*No documentation provided.*

---

### `__gc_unregister_root` (`function`)

*No documentation provided.*

---

### `__gc_unroot_handle` (`function`)

*No documentation provided.*

---

### `__gc_write_barrier` (`function`)

*No documentation provided.*

---

### `__hub_create` (`function`)

*No documentation provided.*

---

### `__hub_register` (`function`)

*No documentation provided.*

---

### `__hub_unregister` (`function`)

*No documentation provided.*

---

### `__hub_wait_receive` (`function`)

*No documentation provided.*

---

### `__hub_wait_receive_index` (`function`)

*No documentation provided.*

---

### `__hub_wait_receive_value` (`function`)

*No documentation provided.*

---

### `__interop_dispatch_ptr` (`function`)

*No documentation provided.*

---

### `__interop_dispatch_unit` (`function`)

*No documentation provided.*

---

### `__interop_dispatch_usize` (`function`)

*No documentation provided.*

---

### `__mutex_create` (`function`)

*No documentation provided.*

---

### `__mutex_lock` (`function`)

*No documentation provided.*

---

### `__mutex_try_lock` (`function`)

*No documentation provided.*

---

### `__mutex_unlock` (`function`)

*No documentation provided.*

---

### `__panic_str` (`function`)

*No documentation provided.*

---

### `__str_len` (`function`)

*No documentation provided.*

---

### `__str_new` (`function`)

*No documentation provided.*

---

### `__syscall_read` (`function`)

*No documentation provided.*

---

### `__syscall_write` (`function`)

*No documentation provided.*

---

### `__test_bytes_len` (`function`)

*No documentation provided.*

---

### `__test_bytes_ptr` (`function`)

*No documentation provided.*

---

### `__wait_group_add` (`function`)

*No documentation provided.*

---

### `__wait_group_create` (`function`)

*No documentation provided.*

---

### `__wait_group_done` (`function`)

*No documentation provided.*

---

### `__wait_group_wait` (`function`)

*No documentation provided.*

---


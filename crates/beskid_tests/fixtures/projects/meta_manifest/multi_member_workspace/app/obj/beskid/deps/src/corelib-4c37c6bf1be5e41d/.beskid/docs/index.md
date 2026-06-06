# API reference

## Structure

- `Ansi`
  - `Contracts`
    - `AnsiCursorStep`
      - `Column`
        - `Ansi::Contracts::AnsiCursorStep::Column` (`contract_method`)
      - `Down`
        - `Ansi::Contracts::AnsiCursorStep::Down` (`contract_method`)
      - `Home`
        - `Ansi::Contracts::AnsiCursorStep::Home` (`contract_method`)
      - `IntoSequence`
        - `Ansi::Contracts::AnsiCursorStep::IntoSequence` (`contract_method`)
      - `Left`
        - `Ansi::Contracts::AnsiCursorStep::Left` (`contract_method`)
      - `NextLine`
        - `Ansi::Contracts::AnsiCursorStep::NextLine` (`contract_method`)
      - `Position`
        - `Ansi::Contracts::AnsiCursorStep::Position` (`contract_method`)
      - `PrevLine`
        - `Ansi::Contracts::AnsiCursorStep::PrevLine` (`contract_method`)
      - `RestoreDec`
        - `Ansi::Contracts::AnsiCursorStep::RestoreDec` (`contract_method`)
      - `Right`
        - `Ansi::Contracts::AnsiCursorStep::Right` (`contract_method`)
      - `SaveDec`
        - `Ansi::Contracts::AnsiCursorStep::SaveDec` (`contract_method`)
      - `Up`
        - `Ansi::Contracts::AnsiCursorStep::Up` (`contract_method`)
      - `col`
        - `Ansi::Contracts::AnsiCursorStep::col` (`parameter`)
        - `Ansi::Contracts::AnsiCursorStep::col` (`parameter`)
      - `count`
        - `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)
        - `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)
        - `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)
        - `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)
        - `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)
        - `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)
      - `row`
        - `Ansi::Contracts::AnsiCursorStep::row` (`parameter`)
      - `Ansi::Contracts::AnsiCursorStep` (`contract`)
    - `AnsiEraseStep`
      - `DisplayAll`
        - `Ansi::Contracts::AnsiEraseStep::DisplayAll` (`contract_method`)
      - `DisplayFromCursor`
        - `Ansi::Contracts::AnsiEraseStep::DisplayFromCursor` (`contract_method`)
      - `DisplaySaved`
        - `Ansi::Contracts::AnsiEraseStep::DisplaySaved` (`contract_method`)
      - `DisplayToCursor`
        - `Ansi::Contracts::AnsiEraseStep::DisplayToCursor` (`contract_method`)
      - `IntoSequence`
        - `Ansi::Contracts::AnsiEraseStep::IntoSequence` (`contract_method`)
      - `LineAll`
        - `Ansi::Contracts::AnsiEraseStep::LineAll` (`contract_method`)
      - `LineFromCursor`
        - `Ansi::Contracts::AnsiEraseStep::LineFromCursor` (`contract_method`)
      - `LineToCursor`
        - `Ansi::Contracts::AnsiEraseStep::LineToCursor` (`contract_method`)
      - `Ansi::Contracts::AnsiEraseStep` (`contract`)
    - `AnsiInputModeStep`
      - `DisableMouseClick`
        - `Ansi::Contracts::AnsiInputModeStep::DisableMouseClick` (`contract_method`)
      - `DisableMouseDrag`
        - `Ansi::Contracts::AnsiInputModeStep::DisableMouseDrag` (`contract_method`)
      - `DisableMouseMotion`
        - `Ansi::Contracts::AnsiInputModeStep::DisableMouseMotion` (`contract_method`)
      - `DisableSgrMouse`
        - `Ansi::Contracts::AnsiInputModeStep::DisableSgrMouse` (`contract_method`)
      - `EnableMouseClick`
        - `Ansi::Contracts::AnsiInputModeStep::EnableMouseClick` (`contract_method`)
      - `EnableMouseDrag`
        - `Ansi::Contracts::AnsiInputModeStep::EnableMouseDrag` (`contract_method`)
      - `EnableMouseMotion`
        - `Ansi::Contracts::AnsiInputModeStep::EnableMouseMotion` (`contract_method`)
      - `EnableSgrMouse`
        - `Ansi::Contracts::AnsiInputModeStep::EnableSgrMouse` (`contract_method`)
      - `IntoSequence`
        - `Ansi::Contracts::AnsiInputModeStep::IntoSequence` (`contract_method`)
      - `RedefineKey`
        - `Ansi::Contracts::AnsiInputModeStep::RedefineKey` (`contract_method`)
      - `binding`
        - `Ansi::Contracts::AnsiInputModeStep::binding` (`parameter`)
      - `code`
        - `Ansi::Contracts::AnsiInputModeStep::code` (`parameter`)
      - `Ansi::Contracts::AnsiInputModeStep` (`contract`)
    - `AnsiOscStep`
      - `Hyperlink`
        - `Ansi::Contracts::AnsiOscStep::Hyperlink` (`contract_method`)
      - `IntoSequence`
        - `Ansi::Contracts::AnsiOscStep::IntoSequence` (`contract_method`)
      - `SetTitle`
        - `Ansi::Contracts::AnsiOscStep::SetTitle` (`contract_method`)
      - `label`
        - `Ansi::Contracts::AnsiOscStep::label` (`parameter`)
      - `title`
        - `Ansi::Contracts::AnsiOscStep::title` (`parameter`)
      - `uri`
        - `Ansi::Contracts::AnsiOscStep::uri` (`parameter`)
      - `Ansi::Contracts::AnsiOscStep` (`contract`)
    - `AnsiScreenStep`
      - `DisableAltScreen`
        - `Ansi::Contracts::AnsiScreenStep::DisableAltScreen` (`contract_method`)
      - `DisableLineWrap`
        - `Ansi::Contracts::AnsiScreenStep::DisableLineWrap` (`contract_method`)
      - `EnableAltScreen`
        - `Ansi::Contracts::AnsiScreenStep::EnableAltScreen` (`contract_method`)
      - `EnableLineWrap`
        - `Ansi::Contracts::AnsiScreenStep::EnableLineWrap` (`contract_method`)
      - `HideCursor`
        - `Ansi::Contracts::AnsiScreenStep::HideCursor` (`contract_method`)
      - `IntoSequence`
        - `Ansi::Contracts::AnsiScreenStep::IntoSequence` (`contract_method`)
      - `RestoreScreen`
        - `Ansi::Contracts::AnsiScreenStep::RestoreScreen` (`contract_method`)
      - `SaveScreen`
        - `Ansi::Contracts::AnsiScreenStep::SaveScreen` (`contract_method`)
      - `ScrollRegion`
        - `Ansi::Contracts::AnsiScreenStep::ScrollRegion` (`contract_method`)
      - `ShowCursor`
        - `Ansi::Contracts::AnsiScreenStep::ShowCursor` (`contract_method`)
      - `bottom`
        - `Ansi::Contracts::AnsiScreenStep::bottom` (`parameter`)
      - `top`
        - `Ansi::Contracts::AnsiScreenStep::top` (`parameter`)
      - `Ansi::Contracts::AnsiScreenStep` (`contract`)
    - `AnsiStyleStep`
      - `ApplyTo`
        - `Ansi::Contracts::AnsiStyleStep::ApplyTo` (`contract_method`)
      - `Bg256`
        - `Ansi::Contracts::AnsiStyleStep::Bg256` (`contract_method`)
      - `BgBasic`
        - `Ansi::Contracts::AnsiStyleStep::BgBasic` (`contract_method`)
      - `BgRgb`
        - `Ansi::Contracts::AnsiStyleStep::BgRgb` (`contract_method`)
      - `Bold`
        - `Ansi::Contracts::AnsiStyleStep::Bold` (`contract_method`)
      - `Dim`
        - `Ansi::Contracts::AnsiStyleStep::Dim` (`contract_method`)
      - `Fg256`
        - `Ansi::Contracts::AnsiStyleStep::Fg256` (`contract_method`)
      - `FgBasic`
        - `Ansi::Contracts::AnsiStyleStep::FgBasic` (`contract_method`)
      - `FgRgb`
        - `Ansi::Contracts::AnsiStyleStep::FgRgb` (`contract_method`)
      - `IntoPrefix`
        - `Ansi::Contracts::AnsiStyleStep::IntoPrefix` (`contract_method`)
      - `Inverse`
        - `Ansi::Contracts::AnsiStyleStep::Inverse` (`contract_method`)
      - `Italic`
        - `Ansi::Contracts::AnsiStyleStep::Italic` (`contract_method`)
      - `Strike`
        - `Ansi::Contracts::AnsiStyleStep::Strike` (`contract_method`)
      - `Underline`
        - `Ansi::Contracts::AnsiStyleStep::Underline` (`contract_method`)
      - `b`
        - `Ansi::Contracts::AnsiStyleStep::b` (`parameter`)
        - `Ansi::Contracts::AnsiStyleStep::b` (`parameter`)
      - `code`
        - `Ansi::Contracts::AnsiStyleStep::code` (`parameter`)
        - `Ansi::Contracts::AnsiStyleStep::code` (`parameter`)
      - `g`
        - `Ansi::Contracts::AnsiStyleStep::g` (`parameter`)
        - `Ansi::Contracts::AnsiStyleStep::g` (`parameter`)
      - `index`
        - `Ansi::Contracts::AnsiStyleStep::index` (`parameter`)
        - `Ansi::Contracts::AnsiStyleStep::index` (`parameter`)
      - `r`
        - `Ansi::Contracts::AnsiStyleStep::r` (`parameter`)
        - `Ansi::Contracts::AnsiStyleStep::r` (`parameter`)
      - `text`
        - `Ansi::Contracts::AnsiStyleStep::text` (`parameter`)
      - `Ansi::Contracts::AnsiStyleStep` (`contract`)
  - `Cursor`
    - `Append`
      - `fragment`
        - `Ansi::Cursor::Append::fragment` (`parameter`)
      - `self`
        - `Ansi::Cursor::Append::self` (`parameter`)
      - `Ansi::Cursor::Append` (`function`)
    - `Column`
      - `col`
        - `Ansi::Cursor::Column::col` (`parameter`)
      - `self`
        - `Ansi::Cursor::Column::self` (`parameter`)
      - `Ansi::Cursor::Column` (`function`)
    - `CursorBuilder`
      - `parts`
        - `Ansi::Cursor::CursorBuilder::parts` (`field`)
      - `Ansi::Cursor::CursorBuilder` (`type`)
    - `Down`
      - `count`
        - `Ansi::Cursor::Down::count` (`parameter`)
      - `self`
        - `Ansi::Cursor::Down::self` (`parameter`)
      - `Ansi::Cursor::Down` (`function`)
    - `Home`
      - `self`
        - `Ansi::Cursor::Home::self` (`parameter`)
      - `Ansi::Cursor::Home` (`function`)
    - `IntoSequence`
      - `self`
        - `Ansi::Cursor::IntoSequence::self` (`parameter`)
      - `Ansi::Cursor::IntoSequence` (`function`)
    - `Left`
      - `count`
        - `Ansi::Cursor::Left::count` (`parameter`)
      - `self`
        - `Ansi::Cursor::Left::self` (`parameter`)
      - `Ansi::Cursor::Left` (`function`)
    - `NextLine`
      - `count`
        - `Ansi::Cursor::NextLine::count` (`parameter`)
      - `self`
        - `Ansi::Cursor::NextLine::self` (`parameter`)
      - `Ansi::Cursor::NextLine` (`function`)
    - `Position`
      - `col`
        - `Ansi::Cursor::Position::col` (`parameter`)
      - `row`
        - `Ansi::Cursor::Position::row` (`parameter`)
      - `self`
        - `Ansi::Cursor::Position::self` (`parameter`)
      - `Ansi::Cursor::Position` (`function`)
    - `PrevLine`
      - `count`
        - `Ansi::Cursor::PrevLine::count` (`parameter`)
      - `self`
        - `Ansi::Cursor::PrevLine::self` (`parameter`)
      - `Ansi::Cursor::PrevLine` (`function`)
    - `RestoreDec`
      - `self`
        - `Ansi::Cursor::RestoreDec::self` (`parameter`)
      - `Ansi::Cursor::RestoreDec` (`function`)
    - `Right`
      - `count`
        - `Ansi::Cursor::Right::count` (`parameter`)
      - `self`
        - `Ansi::Cursor::Right::self` (`parameter`)
      - `Ansi::Cursor::Right` (`function`)
    - `SaveDec`
      - `self`
        - `Ansi::Cursor::SaveDec::self` (`parameter`)
      - `Ansi::Cursor::SaveDec` (`function`)
    - `Start`
      - `Ansi::Cursor::Start` (`function`)
    - `Up`
      - `count`
        - `Ansi::Cursor::Up::count` (`parameter`)
      - `self`
        - `Ansi::Cursor::Up::self` (`parameter`)
      - `Ansi::Cursor::Up` (`function`)
    - `Ansi::Cursor` (`module`)
  - `Erase`
    - `Append`
      - `fragment`
        - `Ansi::Erase::Append::fragment` (`parameter`)
      - `self`
        - `Ansi::Erase::Append::self` (`parameter`)
      - `Ansi::Erase::Append` (`function`)
    - `DisplayAll`
      - `self`
        - `Ansi::Erase::DisplayAll::self` (`parameter`)
      - `Ansi::Erase::DisplayAll` (`function`)
    - `DisplayFromCursor`
      - `self`
        - `Ansi::Erase::DisplayFromCursor::self` (`parameter`)
      - `Ansi::Erase::DisplayFromCursor` (`function`)
    - `DisplaySaved`
      - `self`
        - `Ansi::Erase::DisplaySaved::self` (`parameter`)
      - `Ansi::Erase::DisplaySaved` (`function`)
    - `DisplayToCursor`
      - `self`
        - `Ansi::Erase::DisplayToCursor::self` (`parameter`)
      - `Ansi::Erase::DisplayToCursor` (`function`)
    - `EraseBuilder`
      - `parts`
        - `Ansi::Erase::EraseBuilder::parts` (`field`)
      - `Ansi::Erase::EraseBuilder` (`type`)
    - `IntoSequence`
      - `self`
        - `Ansi::Erase::IntoSequence::self` (`parameter`)
      - `Ansi::Erase::IntoSequence` (`function`)
    - `LineAll`
      - `self`
        - `Ansi::Erase::LineAll::self` (`parameter`)
      - `Ansi::Erase::LineAll` (`function`)
    - `LineFromCursor`
      - `self`
        - `Ansi::Erase::LineFromCursor::self` (`parameter`)
      - `Ansi::Erase::LineFromCursor` (`function`)
    - `LineToCursor`
      - `self`
        - `Ansi::Erase::LineToCursor::self` (`parameter`)
      - `Ansi::Erase::LineToCursor` (`function`)
    - `Start`
      - `Ansi::Erase::Start` (`function`)
    - `Ansi::Erase` (`module`)
  - `Escape`
    - `Csi`
      - `body`
        - `Ansi::Escape::Csi::body` (`parameter`)
      - `finalByte`
        - `Ansi::Escape::Csi::finalByte` (`parameter`)
      - `Ansi::Escape::Csi` (`function`)
    - `CsiOpen`
      - `Ansi::Escape::CsiOpen` (`function`)
    - `CsiSequence`
      - `body`
        - `Ansi::Escape::CsiSequence::body` (`parameter`)
      - `finalByte`
        - `Ansi::Escape::CsiSequence::finalByte` (`parameter`)
      - `Ansi::Escape::CsiSequence` (`function`)
    - `DecRestoreCursor`
      - `Ansi::Escape::DecRestoreCursor` (`function`)
    - `DecSaveCursor`
      - `Ansi::Escape::DecSaveCursor` (`function`)
    - `EmitCsi`
      - `body`
        - `Ansi::Escape::EmitCsi::body` (`parameter`)
      - `finalByte`
        - `Ansi::Escape::EmitCsi::finalByte` (`parameter`)
      - `Ansi::Escape::EmitCsi` (`function`)
    - `EmitDec`
      - `suffix`
        - `Ansi::Escape::EmitDec::suffix` (`parameter`)
      - `Ansi::Escape::EmitDec` (`function`)
    - `EmitOsc`
      - `payload`
        - `Ansi::Escape::EmitOsc::payload` (`parameter`)
      - `Ansi::Escape::EmitOsc` (`function`)
    - `Esc`
      - `Ansi::Escape::Esc` (`function`)
    - `JoinArgs`
      - `a`
        - `Ansi::Escape::JoinArgs::a` (`parameter`)
      - `b`
        - `Ansi::Escape::JoinArgs::b` (`parameter`)
      - `Ansi::Escape::JoinArgs` (`function`)
    - `OscSequence`
      - `payload`
        - `Ansi::Escape::OscSequence::payload` (`parameter`)
      - `Ansi::Escape::OscSequence` (`function`)
    - `PrivateMode`
      - `enable`
        - `Ansi::Escape::PrivateMode::enable` (`parameter`)
      - `mode`
        - `Ansi::Escape::PrivateMode::mode` (`parameter`)
      - `Ansi::Escape::PrivateMode` (`function`)
    - `WhenEnabled`
      - `sequence`
        - `Ansi::Escape::WhenEnabled::sequence` (`parameter`)
      - `Ansi::Escape::WhenEnabled` (`function`)
    - `Ansi::Escape` (`module`)
  - `InputMode`
    - `Append`
      - `fragment`
        - `Ansi::InputMode::Append::fragment` (`parameter`)
      - `self`
        - `Ansi::InputMode::Append::self` (`parameter`)
      - `Ansi::InputMode::Append` (`function`)
    - `DisableMouseClick`
      - `self`
        - `Ansi::InputMode::DisableMouseClick::self` (`parameter`)
      - `Ansi::InputMode::DisableMouseClick` (`function`)
    - `DisableMouseDrag`
      - `self`
        - `Ansi::InputMode::DisableMouseDrag::self` (`parameter`)
      - `Ansi::InputMode::DisableMouseDrag` (`function`)
    - `DisableMouseMotion`
      - `self`
        - `Ansi::InputMode::DisableMouseMotion::self` (`parameter`)
      - `Ansi::InputMode::DisableMouseMotion` (`function`)
    - `DisableSgrMouse`
      - `self`
        - `Ansi::InputMode::DisableSgrMouse::self` (`parameter`)
      - `Ansi::InputMode::DisableSgrMouse` (`function`)
    - `EnableMouseClick`
      - `self`
        - `Ansi::InputMode::EnableMouseClick::self` (`parameter`)
      - `Ansi::InputMode::EnableMouseClick` (`function`)
    - `EnableMouseDrag`
      - `self`
        - `Ansi::InputMode::EnableMouseDrag::self` (`parameter`)
      - `Ansi::InputMode::EnableMouseDrag` (`function`)
    - `EnableMouseMotion`
      - `self`
        - `Ansi::InputMode::EnableMouseMotion::self` (`parameter`)
      - `Ansi::InputMode::EnableMouseMotion` (`function`)
    - `EnableSgrMouse`
      - `self`
        - `Ansi::InputMode::EnableSgrMouse::self` (`parameter`)
      - `Ansi::InputMode::EnableSgrMouse` (`function`)
    - `InputModeBuilder`
      - `parts`
        - `Ansi::InputMode::InputModeBuilder::parts` (`field`)
      - `Ansi::InputMode::InputModeBuilder` (`type`)
    - `IntoSequence`
      - `self`
        - `Ansi::InputMode::IntoSequence::self` (`parameter`)
      - `Ansi::InputMode::IntoSequence` (`function`)
    - `RedefineKey`
      - `binding`
        - `Ansi::InputMode::RedefineKey::binding` (`parameter`)
      - `code`
        - `Ansi::InputMode::RedefineKey::code` (`parameter`)
      - `self`
        - `Ansi::InputMode::RedefineKey::self` (`parameter`)
      - `Ansi::InputMode::RedefineKey` (`function`)
    - `Start`
      - `Ansi::InputMode::Start` (`function`)
    - `Ansi::InputMode` (`module`)
  - `Osc`
    - `Append`
      - `fragment`
        - `Ansi::Osc::Append::fragment` (`parameter`)
      - `self`
        - `Ansi::Osc::Append::self` (`parameter`)
      - `Ansi::Osc::Append` (`function`)
    - `Hyperlink`
      - `label`
        - `Ansi::Osc::Hyperlink::label` (`parameter`)
      - `uri`
        - `Ansi::Osc::Hyperlink::uri` (`parameter`)
      - `Ansi::Osc::Hyperlink` (`function`)
    - `IntoSequence`
      - `self`
        - `Ansi::Osc::IntoSequence::self` (`parameter`)
      - `Ansi::Osc::IntoSequence` (`function`)
    - `OscBuilder`
      - `parts`
        - `Ansi::Osc::OscBuilder::parts` (`field`)
      - `Ansi::Osc::OscBuilder` (`type`)
    - `SetTitle`
      - `self`
        - `Ansi::Osc::SetTitle::self` (`parameter`)
      - `title`
        - `Ansi::Osc::SetTitle::title` (`parameter`)
      - `Ansi::Osc::SetTitle` (`function`)
    - `Start`
      - `Ansi::Osc::Start` (`function`)
    - `Ansi::Osc` (`module`)
  - `Screen`
    - `Append`
      - `fragment`
        - `Ansi::Screen::Append::fragment` (`parameter`)
      - `self`
        - `Ansi::Screen::Append::self` (`parameter`)
      - `Ansi::Screen::Append` (`function`)
    - `DisableAltScreen`
      - `self`
        - `Ansi::Screen::DisableAltScreen::self` (`parameter`)
      - `Ansi::Screen::DisableAltScreen` (`function`)
    - `DisableLineWrap`
      - `self`
        - `Ansi::Screen::DisableLineWrap::self` (`parameter`)
      - `Ansi::Screen::DisableLineWrap` (`function`)
    - `EnableAltScreen`
      - `self`
        - `Ansi::Screen::EnableAltScreen::self` (`parameter`)
      - `Ansi::Screen::EnableAltScreen` (`function`)
    - `EnableLineWrap`
      - `self`
        - `Ansi::Screen::EnableLineWrap::self` (`parameter`)
      - `Ansi::Screen::EnableLineWrap` (`function`)
    - `HideCursor`
      - `self`
        - `Ansi::Screen::HideCursor::self` (`parameter`)
      - `Ansi::Screen::HideCursor` (`function`)
    - `IntoSequence`
      - `self`
        - `Ansi::Screen::IntoSequence::self` (`parameter`)
      - `Ansi::Screen::IntoSequence` (`function`)
    - `RestoreScreen`
      - `self`
        - `Ansi::Screen::RestoreScreen::self` (`parameter`)
      - `Ansi::Screen::RestoreScreen` (`function`)
    - `SaveScreen`
      - `self`
        - `Ansi::Screen::SaveScreen::self` (`parameter`)
      - `Ansi::Screen::SaveScreen` (`function`)
    - `ScreenBuilder`
      - `parts`
        - `Ansi::Screen::ScreenBuilder::parts` (`field`)
      - `Ansi::Screen::ScreenBuilder` (`type`)
    - `ScrollRegion`
      - `bottom`
        - `Ansi::Screen::ScrollRegion::bottom` (`parameter`)
      - `self`
        - `Ansi::Screen::ScrollRegion::self` (`parameter`)
      - `top`
        - `Ansi::Screen::ScrollRegion::top` (`parameter`)
      - `Ansi::Screen::ScrollRegion` (`function`)
    - `ShowCursor`
      - `self`
        - `Ansi::Screen::ShowCursor::self` (`parameter`)
      - `Ansi::Screen::ShowCursor` (`function`)
    - `Start`
      - `Ansi::Screen::Start` (`function`)
    - `Ansi::Screen` (`module`)
  - `Sgr`
    - `ApplyTo`
      - `self`
        - `Ansi::Sgr::ApplyTo::self` (`parameter`)
      - `text`
        - `Ansi::Sgr::ApplyTo::text` (`parameter`)
      - `Ansi::Sgr::ApplyTo` (`function`)
    - `BackgroundColorArgs`
      - `b`
        - `Ansi::Sgr::BackgroundColorArgs::b` (`parameter`)
      - `g`
        - `Ansi::Sgr::BackgroundColorArgs::g` (`parameter`)
      - `r`
        - `Ansi::Sgr::BackgroundColorArgs::r` (`parameter`)
      - `Ansi::Sgr::BackgroundColorArgs` (`function`)
    - `Bg256`
      - `index`
        - `Ansi::Sgr::Bg256::index` (`parameter`)
      - `self`
        - `Ansi::Sgr::Bg256::self` (`parameter`)
      - `Ansi::Sgr::Bg256` (`function`)
    - `BgBasic`
      - `code`
        - `Ansi::Sgr::BgBasic::code` (`parameter`)
      - `self`
        - `Ansi::Sgr::BgBasic::self` (`parameter`)
      - `Ansi::Sgr::BgBasic` (`function`)
    - `BgRgb`
      - `b`
        - `Ansi::Sgr::BgRgb::b` (`parameter`)
      - `g`
        - `Ansi::Sgr::BgRgb::g` (`parameter`)
      - `r`
        - `Ansi::Sgr::BgRgb::r` (`parameter`)
      - `self`
        - `Ansi::Sgr::BgRgb::self` (`parameter`)
      - `Ansi::Sgr::BgRgb` (`function`)
    - `Bold`
      - `self`
        - `Ansi::Sgr::Bold::self` (`parameter`)
      - `Ansi::Sgr::Bold` (`function`)
    - `ClampChannelBucket`
      - `channel`
        - `Ansi::Sgr::ClampChannelBucket::channel` (`parameter`)
      - `Ansi::Sgr::ClampChannelBucket` (`function`)
    - `Dim`
      - `self`
        - `Ansi::Sgr::Dim::self` (`parameter`)
      - `Ansi::Sgr::Dim` (`function`)
    - `DominantChannelIndex`
      - `b`
        - `Ansi::Sgr::DominantChannelIndex::b` (`parameter`)
      - `g`
        - `Ansi::Sgr::DominantChannelIndex::g` (`parameter`)
      - `r`
        - `Ansi::Sgr::DominantChannelIndex::r` (`parameter`)
      - `Ansi::Sgr::DominantChannelIndex` (`function`)
    - `Fg256`
      - `index`
        - `Ansi::Sgr::Fg256::index` (`parameter`)
      - `self`
        - `Ansi::Sgr::Fg256::self` (`parameter`)
      - `Ansi::Sgr::Fg256` (`function`)
    - `FgBasic`
      - `code`
        - `Ansi::Sgr::FgBasic::code` (`parameter`)
      - `self`
        - `Ansi::Sgr::FgBasic::self` (`parameter`)
      - `Ansi::Sgr::FgBasic` (`function`)
    - `FgRgb`
      - `b`
        - `Ansi::Sgr::FgRgb::b` (`parameter`)
      - `g`
        - `Ansi::Sgr::FgRgb::g` (`parameter`)
      - `r`
        - `Ansi::Sgr::FgRgb::r` (`parameter`)
      - `self`
        - `Ansi::Sgr::FgRgb::self` (`parameter`)
      - `Ansi::Sgr::FgRgb` (`function`)
    - `ForegroundColorArgs`
      - `b`
        - `Ansi::Sgr::ForegroundColorArgs::b` (`parameter`)
      - `g`
        - `Ansi::Sgr::ForegroundColorArgs::g` (`parameter`)
      - `r`
        - `Ansi::Sgr::ForegroundColorArgs::r` (`parameter`)
      - `Ansi::Sgr::ForegroundColorArgs` (`function`)
    - `IntoPrefix`
      - `self`
        - `Ansi::Sgr::IntoPrefix::self` (`parameter`)
      - `Ansi::Sgr::IntoPrefix` (`function`)
    - `Inverse`
      - `self`
        - `Ansi::Sgr::Inverse::self` (`parameter`)
      - `Ansi::Sgr::Inverse` (`function`)
    - `Italic`
      - `self`
        - `Ansi::Sgr::Italic::self` (`parameter`)
      - `Ansi::Sgr::Italic` (`function`)
    - `RgbTo256Index`
      - `b`
        - `Ansi::Sgr::RgbTo256Index::b` (`parameter`)
      - `g`
        - `Ansi::Sgr::RgbTo256Index::g` (`parameter`)
      - `r`
        - `Ansi::Sgr::RgbTo256Index::r` (`parameter`)
      - `Ansi::Sgr::RgbTo256Index` (`function`)
    - `RgbToBasicBackground`
      - `b`
        - `Ansi::Sgr::RgbToBasicBackground::b` (`parameter`)
      - `g`
        - `Ansi::Sgr::RgbToBasicBackground::g` (`parameter`)
      - `r`
        - `Ansi::Sgr::RgbToBasicBackground::r` (`parameter`)
      - `Ansi::Sgr::RgbToBasicBackground` (`function`)
    - `RgbToBasicForeground`
      - `b`
        - `Ansi::Sgr::RgbToBasicForeground::b` (`parameter`)
      - `g`
        - `Ansi::Sgr::RgbToBasicForeground::g` (`parameter`)
      - `r`
        - `Ansi::Sgr::RgbToBasicForeground::r` (`parameter`)
      - `Ansi::Sgr::RgbToBasicForeground` (`function`)
    - `SgrBuilder`
      - `openArgs`
        - `Ansi::Sgr::SgrBuilder::openArgs` (`field`)
      - `Ansi::Sgr::SgrBuilder` (`type`)
    - `Start`
      - `Ansi::Sgr::Start` (`function`)
    - `Strike`
      - `self`
        - `Ansi::Sgr::Strike::self` (`parameter`)
      - `Ansi::Sgr::Strike` (`function`)
    - `Underline`
      - `self`
        - `Ansi::Sgr::Underline::self` (`parameter`)
      - `Ansi::Sgr::Underline` (`function`)
    - `Ansi::Sgr` (`module`)
  - `StyleChain`
    - `AppendCode`
      - `chain`
        - `Ansi::StyleChain::AppendCode::chain` (`parameter`)
      - `code`
        - `Ansi::StyleChain::AppendCode::code` (`parameter`)
      - `Ansi::StyleChain::AppendCode` (`function`)
    - `Apply`
      - `chain`
        - `Ansi::StyleChain::Apply::chain` (`parameter`)
      - `text`
        - `Ansi::StyleChain::Apply::text` (`parameter`)
      - `Ansi::StyleChain::Apply` (`function`)
    - `ApplyTo`
      - `chain`
        - `Ansi::StyleChain::ApplyTo::chain` (`parameter`)
      - `text`
        - `Ansi::StyleChain::ApplyTo::text` (`parameter`)
      - `Ansi::StyleChain::ApplyTo` (`function`)
    - `Background256`
      - `chain`
        - `Ansi::StyleChain::Background256::chain` (`parameter`)
      - `index`
        - `Ansi::StyleChain::Background256::index` (`parameter`)
      - `Ansi::StyleChain::Background256` (`function`)
    - `BackgroundRgb`
      - `b`
        - `Ansi::StyleChain::BackgroundRgb::b` (`parameter`)
      - `chain`
        - `Ansi::StyleChain::BackgroundRgb::chain` (`parameter`)
      - `g`
        - `Ansi::StyleChain::BackgroundRgb::g` (`parameter`)
      - `r`
        - `Ansi::StyleChain::BackgroundRgb::r` (`parameter`)
      - `Ansi::StyleChain::BackgroundRgb` (`function`)
    - `Bg256`
      - `chain`
        - `Ansi::StyleChain::Bg256::chain` (`parameter`)
      - `index`
        - `Ansi::StyleChain::Bg256::index` (`parameter`)
      - `Ansi::StyleChain::Bg256` (`function`)
    - `BgBasic`
      - `chain`
        - `Ansi::StyleChain::BgBasic::chain` (`parameter`)
      - `code`
        - `Ansi::StyleChain::BgBasic::code` (`parameter`)
      - `Ansi::StyleChain::BgBasic` (`function`)
    - `BgRgb`
      - `b`
        - `Ansi::StyleChain::BgRgb::b` (`parameter`)
      - `chain`
        - `Ansi::StyleChain::BgRgb::chain` (`parameter`)
      - `g`
        - `Ansi::StyleChain::BgRgb::g` (`parameter`)
      - `r`
        - `Ansi::StyleChain::BgRgb::r` (`parameter`)
      - `Ansi::StyleChain::BgRgb` (`function`)
    - `Bold`
      - `chain`
        - `Ansi::StyleChain::Bold::chain` (`parameter`)
      - `Ansi::StyleChain::Bold` (`function`)
    - `Dim`
      - `chain`
        - `Ansi::StyleChain::Dim::chain` (`parameter`)
      - `Ansi::StyleChain::Dim` (`function`)
    - `Fg256`
      - `chain`
        - `Ansi::StyleChain::Fg256::chain` (`parameter`)
      - `index`
        - `Ansi::StyleChain::Fg256::index` (`parameter`)
      - `Ansi::StyleChain::Fg256` (`function`)
    - `FgBasic`
      - `chain`
        - `Ansi::StyleChain::FgBasic::chain` (`parameter`)
      - `code`
        - `Ansi::StyleChain::FgBasic::code` (`parameter`)
      - `Ansi::StyleChain::FgBasic` (`function`)
    - `FgRgb`
      - `b`
        - `Ansi::StyleChain::FgRgb::b` (`parameter`)
      - `chain`
        - `Ansi::StyleChain::FgRgb::chain` (`parameter`)
      - `g`
        - `Ansi::StyleChain::FgRgb::g` (`parameter`)
      - `r`
        - `Ansi::StyleChain::FgRgb::r` (`parameter`)
      - `Ansi::StyleChain::FgRgb` (`function`)
    - `Foreground256`
      - `chain`
        - `Ansi::StyleChain::Foreground256::chain` (`parameter`)
      - `index`
        - `Ansi::StyleChain::Foreground256::index` (`parameter`)
      - `Ansi::StyleChain::Foreground256` (`function`)
    - `ForegroundRgb`
      - `b`
        - `Ansi::StyleChain::ForegroundRgb::b` (`parameter`)
      - `chain`
        - `Ansi::StyleChain::ForegroundRgb::chain` (`parameter`)
      - `g`
        - `Ansi::StyleChain::ForegroundRgb::g` (`parameter`)
      - `r`
        - `Ansi::StyleChain::ForegroundRgb::r` (`parameter`)
      - `Ansi::StyleChain::ForegroundRgb` (`function`)
    - `IntoPrefix`
      - `chain`
        - `Ansi::StyleChain::IntoPrefix::chain` (`parameter`)
      - `Ansi::StyleChain::IntoPrefix` (`function`)
    - `Inverse`
      - `chain`
        - `Ansi::StyleChain::Inverse::chain` (`parameter`)
      - `Ansi::StyleChain::Inverse` (`function`)
    - `Italic`
      - `chain`
        - `Ansi::StyleChain::Italic::chain` (`parameter`)
      - `Ansi::StyleChain::Italic` (`function`)
    - `New`
      - `Ansi::StyleChain::New` (`function`)
    - `Open`
      - `chain`
        - `Ansi::StyleChain::Open::chain` (`parameter`)
      - `Ansi::StyleChain::Open` (`function`)
    - `Reset`
      - `Ansi::StyleChain::Reset` (`function`)
    - `Strike`
      - `chain`
        - `Ansi::StyleChain::Strike::chain` (`parameter`)
      - `Ansi::StyleChain::Strike` (`function`)
    - `StyleChain`
      - `openCodes`
        - `Ansi::StyleChain::StyleChain::openCodes` (`field`)
      - `Ansi::StyleChain::StyleChain` (`type`)
    - `Underline`
      - `chain`
        - `Ansi::StyleChain::Underline::chain` (`parameter`)
      - `Ansi::StyleChain::Underline` (`function`)
    - `Ansi::StyleChain` (`module`)
- `Beskid`
  - `Compiler`
    - `Collect`
      - `AnalysisRequest`
        - `Beskid::Compiler::Collect::AnalysisRequest` (`type`)
      - `AnalysisResult`
        - `Beskid::Compiler::Collect::AnalysisResult` (`type`)
      - `Analyzer`
        - `Analyze`
          - `Beskid::Compiler::Collect::Analyzer::Analyze` (`contract_method`)
        - `request`
          - `Beskid::Compiler::Collect::Analyzer::request` (`parameter`)
        - `Beskid::Compiler::Collect::Analyzer` (`contract`)
      - `AttributeDeclarationSet`
        - `Beskid::Compiler::Collect::AttributeDeclarationSet` (`type`)
      - `AttributeGenerationRequest`
        - `Beskid::Compiler::Collect::AttributeGenerationRequest` (`type`)
      - `AttributeGenerator`
        - `Attributes`
          - `Beskid::Compiler::Collect::AttributeGenerator::Attributes` (`contract_method`)
        - `request`
          - `Beskid::Compiler::Collect::AttributeGenerator::request` (`parameter`)
        - `Beskid::Compiler::Collect::AttributeGenerator` (`contract`)
      - `CollectFacadeVersion`
        - `Beskid::Compiler::Collect::CollectFacadeVersion` (`function`)
      - `CollectRequest`
        - `Beskid::Compiler::Collect::CollectRequest` (`type`)
      - `CollectTargetSet`
        - `Beskid::Compiler::Collect::CollectTargetSet` (`type`)
      - `Collector`
        - `Collect`
          - `Beskid::Compiler::Collect::Collector::Collect` (`contract_method`)
        - `request`
          - `Beskid::Compiler::Collect::Collector::request` (`parameter`)
        - `Beskid::Compiler::Collect::Collector` (`contract`)
      - `FixError`
        - `Beskid::Compiler::Collect::FixError` (`type`)
      - `GeneratedSyntaxContribution`
        - `Beskid::Compiler::Collect::GeneratedSyntaxContribution` (`type`)
      - `GenerationRequest`
        - `Beskid::Compiler::Collect::GenerationRequest` (`type`)
      - `Generator`
        - `Generate`
          - `Beskid::Compiler::Collect::Generator::Generate` (`contract_method`)
        - `request`
          - `Beskid::Compiler::Collect::Generator::request` (`parameter`)
        - `Beskid::Compiler::Collect::Generator` (`contract`)
      - `Rewriter`
        - `Rewrite`
          - `Beskid::Compiler::Collect::Rewriter::Rewrite` (`contract_method`)
        - `sourceNode`
          - `Beskid::Compiler::Collect::Rewriter::sourceNode` (`parameter`)
        - `Beskid::Compiler::Collect::Rewriter` (`contract`)
      - `Beskid::Compiler::Collect` (`module`)
    - `Compilation`
      - `CompilerLanguageVersionToken`
        - `Beskid::Compiler::Compilation::CompilerLanguageVersionToken` (`function`)
      - `ModSdkCompilationSurfaceVersion`
        - `Beskid::Compiler::Compilation::ModSdkCompilationSurfaceVersion` (`function`)
      - `ReflectSdkCompilationSnapshotPlane`
        - `Hir`
          - `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::Hir` (`enum_variant`)
        - `SemanticSnapshot`
          - `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::SemanticSnapshot` (`enum_variant`)
        - `SyntaxTree`
          - `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::SyntaxTree` (`enum_variant`)
        - `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane` (`enum`)
      - `SemanticSnapshotFamilyToken`
        - `Beskid::Compiler::Compilation::SemanticSnapshotFamilyToken` (`function`)
      - `Beskid::Compiler::Compilation` (`module`)
    - `Diagnostics`
      - `Beskid::Compiler::Diagnostics` (`module`)
    - `Query`
      - `Beskid::Compiler::Query` (`module`)
    - `TypedEmitter`
      - `ReflectSdkEmitContributionKind`
        - `DiagnosticAttachment`
          - `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::DiagnosticAttachment` (`enum_variant`)
        - `LoweringDirective`
          - `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::LoweringDirective` (`enum_variant`)
        - `Metadata`
          - `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::Metadata` (`enum_variant`)
        - `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind` (`enum`)
      - `TypedEmitterFacadeVersion`
        - `Beskid::Compiler::TypedEmitter::TypedEmitterFacadeVersion` (`function`)
      - `Beskid::Compiler::TypedEmitter` (`module`)
  - `Syntax`
    - `Nodes`
      - `ArrayLiteralExpression`
        - `ArrayLiteralExpression`
          - `elements`
            - `Beskid::Syntax::Nodes::ArrayLiteralExpression::ArrayLiteralExpression::elements` (`field`)
          - `Beskid::Syntax::Nodes::ArrayLiteralExpression::ArrayLiteralExpression` (`type`)
        - `Beskid::Syntax::Nodes::ArrayLiteralExpression` (`module`)
      - `AssignExpression`
        - `AssignExpression`
          - `target`
            - `Beskid::Syntax::Nodes::AssignExpression::AssignExpression::target` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::AssignExpression::AssignExpression::value` (`field`)
          - `Beskid::Syntax::Nodes::AssignExpression::AssignExpression` (`type`)
        - `Beskid::Syntax::Nodes::AssignExpression` (`module`)
      - `AssignOp`
        - `AssignOp`
          - `AddAssign`
            - `Beskid::Syntax::Nodes::AssignOp::AssignOp::AddAssign` (`enum_variant`)
          - `Assign`
            - `Beskid::Syntax::Nodes::AssignOp::AssignOp::Assign` (`enum_variant`)
          - `SubAssign`
            - `Beskid::Syntax::Nodes::AssignOp::AssignOp::SubAssign` (`enum_variant`)
          - `Beskid::Syntax::Nodes::AssignOp::AssignOp` (`enum`)
        - `Beskid::Syntax::Nodes::AssignOp` (`module`)
      - `Attribute`
        - `Attribute`
          - `arguments`
            - `Beskid::Syntax::Nodes::Attribute::Attribute::arguments` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::Attribute::Attribute::name` (`field`)
          - `Beskid::Syntax::Nodes::Attribute::Attribute` (`type`)
        - `Beskid::Syntax::Nodes::Attribute` (`module`)
      - `AttributeArgument`
        - `AttributeArgument`
          - `name`
            - `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument::name` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument::value` (`field`)
          - `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument` (`type`)
        - `Beskid::Syntax::Nodes::AttributeArgument` (`module`)
      - `AttributeArgumentList`
        - `Beskid::Syntax::Nodes::AttributeArgumentList` (`module`)
      - `AttributeDeclaration`
        - `AttributeDeclaration`
          - `name`
            - `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::parameters` (`field`)
          - `targets`
            - `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::targets` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::visibility` (`field`)
          - `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration` (`type`)
        - `Beskid::Syntax::Nodes::AttributeDeclaration` (`module`)
      - `AttributeList`
        - `Beskid::Syntax::Nodes::AttributeList` (`module`)
      - `AttributeParameter`
        - `AttributeParameter`
          - `defaultValue`
            - `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::defaultValue` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::name` (`field`)
          - `ty`
            - `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::ty` (`field`)
          - `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter` (`type`)
        - `Beskid::Syntax::Nodes::AttributeParameter` (`module`)
      - `AttributeParameterList`
        - `Beskid::Syntax::Nodes::AttributeParameterList` (`module`)
      - `AttributeTarget`
        - `AttributeTarget`
          - `name`
            - `Beskid::Syntax::Nodes::AttributeTarget::AttributeTarget::name` (`field`)
          - `Beskid::Syntax::Nodes::AttributeTarget::AttributeTarget` (`type`)
        - `Beskid::Syntax::Nodes::AttributeTarget` (`module`)
      - `AttributeTargetList`
        - `Beskid::Syntax::Nodes::AttributeTargetList` (`module`)
      - `BinaryExpression`
        - `BinaryExpression`
          - `left`
            - `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::left` (`field`)
          - `op`
            - `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::op` (`field`)
          - `right`
            - `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::right` (`field`)
          - `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression` (`type`)
        - `Beskid::Syntax::Nodes::BinaryExpression` (`module`)
      - `BinaryOp`
        - `BinaryOp`
          - `Add`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Add` (`enum_variant`)
          - `And`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::And` (`enum_variant`)
          - `Div`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Div` (`enum_variant`)
          - `Eq`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Eq` (`enum_variant`)
          - `Gt`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Gt` (`enum_variant`)
          - `Gte`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Gte` (`enum_variant`)
          - `IdentityEq`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::IdentityEq` (`enum_variant`)
          - `IdentityNotEq`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::IdentityNotEq` (`enum_variant`)
          - `Lt`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Lt` (`enum_variant`)
          - `Lte`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Lte` (`enum_variant`)
          - `Mul`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Mul` (`enum_variant`)
          - `NotEq`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::NotEq` (`enum_variant`)
          - `Or`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Or` (`enum_variant`)
          - `Sub`
            - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Sub` (`enum_variant`)
          - `Beskid::Syntax::Nodes::BinaryOp::BinaryOp` (`enum`)
        - `Beskid::Syntax::Nodes::BinaryOp` (`module`)
      - `Block`
        - `Block`
          - `statements`
            - `Beskid::Syntax::Nodes::Block::Block::statements` (`field`)
          - `Beskid::Syntax::Nodes::Block::Block` (`type`)
        - `Beskid::Syntax::Nodes::Block` (`module`)
      - `BlockExpression`
        - `BlockExpression`
          - `block`
            - `Beskid::Syntax::Nodes::BlockExpression::BlockExpression::block` (`field`)
          - `Beskid::Syntax::Nodes::BlockExpression::BlockExpression` (`type`)
        - `Beskid::Syntax::Nodes::BlockExpression` (`module`)
      - `BreakStatement`
        - `BreakStatement`
          - `Beskid::Syntax::Nodes::BreakStatement::BreakStatement` (`type`)
        - `Beskid::Syntax::Nodes::BreakStatement` (`module`)
      - `CallExpression`
        - `CallExpression`
          - `args`
            - `Beskid::Syntax::Nodes::CallExpression::CallExpression::args` (`field`)
          - `callee`
            - `Beskid::Syntax::Nodes::CallExpression::CallExpression::callee` (`field`)
          - `Beskid::Syntax::Nodes::CallExpression::CallExpression` (`type`)
        - `Beskid::Syntax::Nodes::CallExpression` (`module`)
      - `ContinueStatement`
        - `ContinueStatement`
          - `Beskid::Syntax::Nodes::ContinueStatement::ContinueStatement` (`type`)
        - `Beskid::Syntax::Nodes::ContinueStatement` (`module`)
      - `ContractDefinition`
        - `ContractDefinition`
          - `attributes`
            - `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::attributes` (`field`)
          - `items`
            - `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::items` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::name` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition` (`type`)
        - `Beskid::Syntax::Nodes::ContractDefinition` (`module`)
      - `ContractEmbedding`
        - `ContractEmbedding`
          - `name`
            - `Beskid::Syntax::Nodes::ContractEmbedding::ContractEmbedding::name` (`field`)
          - `Beskid::Syntax::Nodes::ContractEmbedding::ContractEmbedding` (`type`)
        - `Beskid::Syntax::Nodes::ContractEmbedding` (`module`)
      - `ContractMethodSignature`
        - `ContractMethodSignature`
          - `name`
            - `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::parameters` (`field`)
          - `returnType`
            - `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::returnType` (`field`)
          - `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature` (`type`)
        - `Beskid::Syntax::Nodes::ContractMethodSignature` (`module`)
      - `ContractNode`
        - `ContractNode`
          - `Embedding`
            - `Beskid::Syntax::Nodes::ContractNode::ContractNode::Embedding` (`enum_variant`)
          - `MethodSignature`
            - `Beskid::Syntax::Nodes::ContractNode::ContractNode::MethodSignature` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::ContractNode::ContractNode::payload` (`field`)
            - `Beskid::Syntax::Nodes::ContractNode::ContractNode::payload` (`field`)
          - `Beskid::Syntax::Nodes::ContractNode::ContractNode` (`enum`)
        - `Beskid::Syntax::Nodes::ContractNode` (`module`)
      - `ContractNodeList`
        - `Beskid::Syntax::Nodes::ContractNodeList` (`module`)
      - `Descendants`
        - `Descendants`
          - `Current`
            - `Beskid::Syntax::Nodes::Descendants::Descendants::Current` (`contract_method`)
          - `MoveNext`
            - `Beskid::Syntax::Nodes::Descendants::Descendants::MoveNext` (`contract_method`)
          - `Beskid::Syntax::Nodes::Descendants::Descendants` (`contract`)
        - `Beskid::Syntax::Nodes::Descendants` (`module`)
      - `EnumConstructorExpression`
        - `EnumConstructorExpression`
          - `args`
            - `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression::args` (`field`)
          - `path`
            - `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression::path` (`field`)
          - `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression` (`type`)
        - `Beskid::Syntax::Nodes::EnumConstructorExpression` (`module`)
      - `EnumDefinition`
        - `EnumDefinition`
          - `generics`
            - `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::generics` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::name` (`field`)
          - `variants`
            - `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::variants` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition` (`type`)
        - `Beskid::Syntax::Nodes::EnumDefinition` (`module`)
      - `EnumPath`
        - `EnumPath`
          - `typePath`
            - `Beskid::Syntax::Nodes::EnumPath::EnumPath::typePath` (`field`)
          - `variant`
            - `Beskid::Syntax::Nodes::EnumPath::EnumPath::variant` (`field`)
          - `Beskid::Syntax::Nodes::EnumPath::EnumPath` (`type`)
        - `Beskid::Syntax::Nodes::EnumPath` (`module`)
      - `EnumPattern`
        - `EnumPattern`
          - `items`
            - `Beskid::Syntax::Nodes::EnumPattern::EnumPattern::items` (`field`)
          - `path`
            - `Beskid::Syntax::Nodes::EnumPattern::EnumPattern::path` (`field`)
          - `Beskid::Syntax::Nodes::EnumPattern::EnumPattern` (`type`)
        - `Beskid::Syntax::Nodes::EnumPattern` (`module`)
      - `EnumVariant`
        - `EnumVariant`
          - `fields`
            - `Beskid::Syntax::Nodes::EnumVariant::EnumVariant::fields` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::EnumVariant::EnumVariant::name` (`field`)
          - `Beskid::Syntax::Nodes::EnumVariant::EnumVariant` (`type`)
        - `Beskid::Syntax::Nodes::EnumVariant` (`module`)
      - `EnumVariantList`
        - `Beskid::Syntax::Nodes::EnumVariantList` (`module`)
      - `Expression`
        - `Expression`
          - `ArrayLiteral`
            - `Beskid::Syntax::Nodes::Expression::Expression::ArrayLiteral` (`enum_variant`)
          - `Assign`
            - `Beskid::Syntax::Nodes::Expression::Expression::Assign` (`enum_variant`)
          - `Binary`
            - `Beskid::Syntax::Nodes::Expression::Expression::Binary` (`enum_variant`)
          - `Block`
            - `Beskid::Syntax::Nodes::Expression::Expression::Block` (`enum_variant`)
          - `Call`
            - `Beskid::Syntax::Nodes::Expression::Expression::Call` (`enum_variant`)
          - `EnumConstructor`
            - `Beskid::Syntax::Nodes::Expression::Expression::EnumConstructor` (`enum_variant`)
          - `Grouped`
            - `Beskid::Syntax::Nodes::Expression::Expression::Grouped` (`enum_variant`)
          - `Index`
            - `Beskid::Syntax::Nodes::Expression::Expression::Index` (`enum_variant`)
          - `Lambda`
            - `Beskid::Syntax::Nodes::Expression::Expression::Lambda` (`enum_variant`)
          - `Literal`
            - `Beskid::Syntax::Nodes::Expression::Expression::Literal` (`enum_variant`)
          - `MacroInvocation`
            - `Beskid::Syntax::Nodes::Expression::Expression::MacroInvocation` (`enum_variant`)
          - `MacroMetavariable`
            - `Beskid::Syntax::Nodes::Expression::Expression::MacroMetavariable` (`enum_variant`)
          - `Match`
            - `Beskid::Syntax::Nodes::Expression::Expression::Match` (`enum_variant`)
          - `Member`
            - `Beskid::Syntax::Nodes::Expression::Expression::Member` (`enum_variant`)
          - `Path`
            - `Beskid::Syntax::Nodes::Expression::Expression::Path` (`enum_variant`)
          - `Spawn`
            - `Beskid::Syntax::Nodes::Expression::Expression::Spawn` (`enum_variant`)
          - `StructLiteral`
            - `Beskid::Syntax::Nodes::Expression::Expression::StructLiteral` (`enum_variant`)
          - `Try`
            - `Beskid::Syntax::Nodes::Expression::Expression::Try` (`enum_variant`)
          - `Unary`
            - `Beskid::Syntax::Nodes::Expression::Expression::Unary` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
            - `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)
          - `Beskid::Syntax::Nodes::Expression::Expression` (`enum`)
        - `Beskid::Syntax::Nodes::Expression` (`module`)
      - `ExpressionList`
        - `Beskid::Syntax::Nodes::ExpressionList` (`module`)
      - `ExpressionStatement`
        - `ExpressionStatement`
          - `expression`
            - `Beskid::Syntax::Nodes::ExpressionStatement::ExpressionStatement::expression` (`field`)
          - `Beskid::Syntax::Nodes::ExpressionStatement::ExpressionStatement` (`type`)
        - `Beskid::Syntax::Nodes::ExpressionStatement` (`module`)
      - `ExtendTypeDefinition`
        - `ExtendTypeDefinition`
          - `methods`
            - `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition::methods` (`field`)
          - `targetType`
            - `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition::targetType` (`field`)
          - `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition` (`type`)
        - `Beskid::Syntax::Nodes::ExtendTypeDefinition` (`module`)
      - `Field`
        - `Field`
          - `name`
            - `Beskid::Syntax::Nodes::Field::Field::name` (`field`)
          - `ty`
            - `Beskid::Syntax::Nodes::Field::Field::ty` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::Field::Field::visibility` (`field`)
          - `Beskid::Syntax::Nodes::Field::Field` (`type`)
        - `Beskid::Syntax::Nodes::Field` (`module`)
      - `FieldKind`
        - `FieldKind`
          - `Event`
            - `Beskid::Syntax::Nodes::FieldKind::FieldKind::Event` (`enum_variant`)
          - `Injected`
            - `Beskid::Syntax::Nodes::FieldKind::FieldKind::Injected` (`enum_variant`)
          - `Value`
            - `Beskid::Syntax::Nodes::FieldKind::FieldKind::Value` (`enum_variant`)
          - `Beskid::Syntax::Nodes::FieldKind::FieldKind` (`enum`)
        - `Beskid::Syntax::Nodes::FieldKind` (`module`)
      - `FieldList`
        - `Beskid::Syntax::Nodes::FieldList` (`module`)
      - `ForStatement`
        - `ForStatement`
          - `body`
            - `Beskid::Syntax::Nodes::ForStatement::ForStatement::body` (`field`)
          - `iterable`
            - `Beskid::Syntax::Nodes::ForStatement::ForStatement::iterable` (`field`)
          - `iterator`
            - `Beskid::Syntax::Nodes::ForStatement::ForStatement::iterator` (`field`)
          - `Beskid::Syntax::Nodes::ForStatement::ForStatement` (`type`)
        - `Beskid::Syntax::Nodes::ForStatement` (`module`)
      - `FunctionDefinition`
        - `FunctionDefinition`
          - `attributes`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::attributes` (`field`)
          - `body`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::body` (`field`)
          - `generics`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::generics` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::parameters` (`field`)
          - `returnType`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::returnType` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition` (`type`)
        - `Beskid::Syntax::Nodes::FunctionDefinition` (`module`)
      - `GroupedExpression`
        - `GroupedExpression`
          - `expr`
            - `Beskid::Syntax::Nodes::GroupedExpression::GroupedExpression::expr` (`field`)
          - `Beskid::Syntax::Nodes::GroupedExpression::GroupedExpression` (`type`)
        - `Beskid::Syntax::Nodes::GroupedExpression` (`module`)
      - `HostBodyItem`
        - `Beskid::Syntax::Nodes::HostBodyItem` (`module`)
      - `HostBodyItemList`
        - `Beskid::Syntax::Nodes::HostBodyItemList` (`module`)
      - `HostDefinition`
        - `HostDefinition`
          - `baseHost`
            - `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::baseHost` (`field`)
          - `body`
            - `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::body` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::parameters` (`field`)
          - `Beskid::Syntax::Nodes::HostDefinition::HostDefinition` (`type`)
        - `Beskid::Syntax::Nodes::HostDefinition` (`module`)
      - `Identifier`
        - `Identifier`
          - `name`
            - `Beskid::Syntax::Nodes::Identifier::Identifier::name` (`field`)
          - `Beskid::Syntax::Nodes::Identifier::Identifier` (`type`)
        - `Beskid::Syntax::Nodes::Identifier` (`module`)
      - `IdentifierList`
        - `Beskid::Syntax::Nodes::IdentifierList` (`module`)
      - `IfStatement`
        - `IfStatement`
          - `condition`
            - `Beskid::Syntax::Nodes::IfStatement::IfStatement::condition` (`field`)
          - `elseBlock`
            - `Beskid::Syntax::Nodes::IfStatement::IfStatement::elseBlock` (`field`)
          - `thenBlock`
            - `Beskid::Syntax::Nodes::IfStatement::IfStatement::thenBlock` (`field`)
          - `Beskid::Syntax::Nodes::IfStatement::IfStatement` (`type`)
        - `Beskid::Syntax::Nodes::IfStatement` (`module`)
      - `IndexExpression`
        - `IndexExpression`
          - `index`
            - `Beskid::Syntax::Nodes::IndexExpression::IndexExpression::index` (`field`)
          - `target`
            - `Beskid::Syntax::Nodes::IndexExpression::IndexExpression::target` (`field`)
          - `Beskid::Syntax::Nodes::IndexExpression::IndexExpression` (`type`)
        - `Beskid::Syntax::Nodes::IndexExpression` (`module`)
      - `InjectQualifier`
        - `InjectQualifier`
          - `Global`
            - `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier::Global` (`enum_variant`)
          - `Parent`
            - `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier::Parent` (`enum_variant`)
          - `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier` (`enum`)
        - `Beskid::Syntax::Nodes::InjectQualifier` (`module`)
      - `InlineModule`
        - `InlineModule`
          - `attributes`
            - `Beskid::Syntax::Nodes::InlineModule::InlineModule::attributes` (`field`)
          - `items`
            - `Beskid::Syntax::Nodes::InlineModule::InlineModule::items` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::InlineModule::InlineModule::name` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::InlineModule::InlineModule::visibility` (`field`)
          - `Beskid::Syntax::Nodes::InlineModule::InlineModule` (`type`)
        - `Beskid::Syntax::Nodes::InlineModule` (`module`)
      - `LambdaExpression`
        - `LambdaExpression`
          - `body`
            - `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression::body` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression::parameters` (`field`)
          - `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression` (`type`)
        - `Beskid::Syntax::Nodes::LambdaExpression` (`module`)
      - `LambdaParameter`
        - `LambdaParameter`
          - `name`
            - `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter::name` (`field`)
          - `ty`
            - `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter::ty` (`field`)
          - `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter` (`type`)
        - `Beskid::Syntax::Nodes::LambdaParameter` (`module`)
      - `LambdaParameterList`
        - `Beskid::Syntax::Nodes::LambdaParameterList` (`module`)
      - `LaunchStatement`
        - `LaunchStatement`
          - `arguments`
            - `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement::arguments` (`field`)
          - `hostPath`
            - `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement::hostPath` (`field`)
          - `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement` (`type`)
        - `Beskid::Syntax::Nodes::LaunchStatement` (`module`)
      - `LetStatement`
        - `LetStatement`
          - `name`
            - `Beskid::Syntax::Nodes::LetStatement::LetStatement::name` (`field`)
          - `typeAnnotation`
            - `Beskid::Syntax::Nodes::LetStatement::LetStatement::typeAnnotation` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::LetStatement::LetStatement::value` (`field`)
          - `Beskid::Syntax::Nodes::LetStatement::LetStatement` (`type`)
        - `Beskid::Syntax::Nodes::LetStatement` (`module`)
      - `Literal`
        - `Literal`
          - `Bool`
            - `Beskid::Syntax::Nodes::Literal::Literal::Bool` (`enum_variant`)
          - `Char`
            - `Beskid::Syntax::Nodes::Literal::Literal::Char` (`enum_variant`)
          - `Float`
            - `Beskid::Syntax::Nodes::Literal::Literal::Float` (`enum_variant`)
          - `Integer`
            - `Beskid::Syntax::Nodes::Literal::Literal::Integer` (`enum_variant`)
          - `String`
            - `Beskid::Syntax::Nodes::Literal::Literal::String` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)
            - `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)
            - `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)
            - `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)
            - `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)
          - `Beskid::Syntax::Nodes::Literal::Literal` (`enum`)
        - `Beskid::Syntax::Nodes::Literal` (`module`)
      - `LiteralExpression`
        - `LiteralExpression`
          - `literal`
            - `Beskid::Syntax::Nodes::LiteralExpression::LiteralExpression::literal` (`field`)
          - `Beskid::Syntax::Nodes::LiteralExpression::LiteralExpression` (`type`)
        - `Beskid::Syntax::Nodes::LiteralExpression` (`module`)
      - `MacroDefinition`
        - `MacroDefinition`
          - `body`
            - `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::body` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::parameters` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition` (`type`)
        - `Beskid::Syntax::Nodes::MacroDefinition` (`module`)
      - `MacroFragmentKind`
        - `MacroFragmentKind`
          - `Block`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Block` (`enum_variant`)
          - `Expression`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Expression` (`enum_variant`)
          - `Identifier`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Identifier` (`enum_variant`)
          - `Item`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Item` (`enum_variant`)
          - `Literal`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Literal` (`enum_variant`)
          - `Node`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Node` (`enum_variant`)
          - `Path`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Path` (`enum_variant`)
          - `Pattern`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Pattern` (`enum_variant`)
          - `Statement`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Statement` (`enum_variant`)
          - `Type`
            - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Type` (`enum_variant`)
          - `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind` (`enum`)
        - `Beskid::Syntax::Nodes::MacroFragmentKind` (`module`)
      - `MacroInvocation`
        - `MacroInvocation`
          - `arguments`
            - `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::arguments` (`field`)
          - `block`
            - `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::block` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::name` (`field`)
          - `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation` (`type`)
        - `Beskid::Syntax::Nodes::MacroInvocation` (`module`)
      - `MacroMetavariable`
        - `MacroMetavariable`
          - `name`
            - `Beskid::Syntax::Nodes::MacroMetavariable::MacroMetavariable::name` (`field`)
          - `Beskid::Syntax::Nodes::MacroMetavariable::MacroMetavariable` (`type`)
        - `Beskid::Syntax::Nodes::MacroMetavariable` (`module`)
      - `MacroParameter`
        - `MacroParameter`
          - `kind`
            - `Beskid::Syntax::Nodes::MacroParameter::MacroParameter::kind` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::MacroParameter::MacroParameter::name` (`field`)
          - `Beskid::Syntax::Nodes::MacroParameter::MacroParameter` (`type`)
        - `Beskid::Syntax::Nodes::MacroParameter` (`module`)
      - `MacroParameterList`
        - `Beskid::Syntax::Nodes::MacroParameterList` (`module`)
      - `MatchArm`
        - `MatchArm`
          - `guard`
            - `Beskid::Syntax::Nodes::MatchArm::MatchArm::guard` (`field`)
          - `pattern`
            - `Beskid::Syntax::Nodes::MatchArm::MatchArm::pattern` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::MatchArm::MatchArm::value` (`field`)
          - `Beskid::Syntax::Nodes::MatchArm::MatchArm` (`type`)
        - `Beskid::Syntax::Nodes::MatchArm` (`module`)
      - `MatchArmList`
        - `Beskid::Syntax::Nodes::MatchArmList` (`module`)
      - `MatchExpression`
        - `MatchExpression`
          - `arms`
            - `Beskid::Syntax::Nodes::MatchExpression::MatchExpression::arms` (`field`)
          - `scrutinee`
            - `Beskid::Syntax::Nodes::MatchExpression::MatchExpression::scrutinee` (`field`)
          - `Beskid::Syntax::Nodes::MatchExpression::MatchExpression` (`type`)
        - `Beskid::Syntax::Nodes::MatchExpression` (`module`)
      - `MemberExpression`
        - `MemberExpression`
          - `member`
            - `Beskid::Syntax::Nodes::MemberExpression::MemberExpression::member` (`field`)
          - `target`
            - `Beskid::Syntax::Nodes::MemberExpression::MemberExpression::target` (`field`)
          - `Beskid::Syntax::Nodes::MemberExpression::MemberExpression` (`type`)
        - `Beskid::Syntax::Nodes::MemberExpression` (`module`)
      - `MethodDefinition`
        - `MethodDefinition`
          - `body`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::body` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::parameters` (`field`)
          - `receiverType`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::receiverType` (`field`)
          - `returnType`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::returnType` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition` (`type`)
        - `Beskid::Syntax::Nodes::MethodDefinition` (`module`)
      - `MethodDefinitionList`
        - `Beskid::Syntax::Nodes::MethodDefinitionList` (`module`)
      - `ModuleDeclaration`
        - `ModuleDeclaration`
          - `attributes`
            - `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::attributes` (`field`)
          - `path`
            - `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::path` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::visibility` (`field`)
          - `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration` (`type`)
        - `Beskid::Syntax::Nodes::ModuleDeclaration` (`module`)
      - `Node`
        - `Node`
          - `Kind`
            - `Beskid::Syntax::Nodes::Node::Node::Kind` (`contract_method`)
          - `PushChildren`
            - `Beskid::Syntax::Nodes::Node::Node::PushChildren` (`contract_method`)
          - `Ref`
            - `Beskid::Syntax::Nodes::Node::Node::Ref` (`contract_method`)
          - `Span`
            - `Beskid::Syntax::Nodes::Node::Node::Span` (`contract_method`)
          - `sink`
            - `Beskid::Syntax::Nodes::Node::Node::sink` (`parameter`)
          - `Beskid::Syntax::Nodes::Node::Node` (`contract`)
        - `NodeChildSink`
          - `Push`
            - `Beskid::Syntax::Nodes::Node::NodeChildSink::Push` (`contract_method`)
          - `child`
            - `Beskid::Syntax::Nodes::Node::NodeChildSink::child` (`parameter`)
          - `Beskid::Syntax::Nodes::Node::NodeChildSink` (`contract`)
        - `Beskid::Syntax::Nodes::Node` (`module`)
      - `NodeKind`
        - `NodeKind`
          - `AssignExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::AssignExpression` (`enum_variant`)
          - `Attribute`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Attribute` (`enum_variant`)
          - `AttributeArgument`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeArgument` (`enum_variant`)
          - `AttributeDeclaration`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeDeclaration` (`enum_variant`)
          - `AttributeParameter`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeParameter` (`enum_variant`)
          - `AttributeTarget`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeTarget` (`enum_variant`)
          - `BinaryExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::BinaryExpression` (`enum_variant`)
          - `BinaryOp`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::BinaryOp` (`enum_variant`)
          - `Block`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Block` (`enum_variant`)
          - `BlockExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::BlockExpression` (`enum_variant`)
          - `BreakStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::BreakStatement` (`enum_variant`)
          - `CallExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::CallExpression` (`enum_variant`)
          - `ContinueStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContinueStatement` (`enum_variant`)
          - `ContractDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractDefinition` (`enum_variant`)
          - `ContractEmbedding`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractEmbedding` (`enum_variant`)
          - `ContractMethodSignature`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractMethodSignature` (`enum_variant`)
          - `ContractNode`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractNode` (`enum_variant`)
          - `EnumConstructorExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumConstructorExpression` (`enum_variant`)
          - `EnumDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumDefinition` (`enum_variant`)
          - `EnumPath`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumPath` (`enum_variant`)
          - `EnumPattern`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumPattern` (`enum_variant`)
          - `EnumVariant`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumVariant` (`enum_variant`)
          - `Expression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Expression` (`enum_variant`)
          - `ExpressionStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ExpressionStatement` (`enum_variant`)
          - `ExtendTypeDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ExtendTypeDefinition` (`enum_variant`)
          - `Field`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Field` (`enum_variant`)
          - `ForStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ForStatement` (`enum_variant`)
          - `FunctionDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::FunctionDefinition` (`enum_variant`)
          - `GroupedExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::GroupedExpression` (`enum_variant`)
          - `HostBodyItem`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::HostBodyItem` (`enum_variant`)
          - `HostDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::HostDefinition` (`enum_variant`)
          - `Identifier`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Identifier` (`enum_variant`)
          - `IfStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::IfStatement` (`enum_variant`)
          - `InlineModule`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::InlineModule` (`enum_variant`)
          - `LambdaExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::LambdaExpression` (`enum_variant`)
          - `LambdaParameter`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::LambdaParameter` (`enum_variant`)
          - `LaunchStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::LaunchStatement` (`enum_variant`)
          - `LetStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::LetStatement` (`enum_variant`)
          - `Literal`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Literal` (`enum_variant`)
          - `LiteralExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::LiteralExpression` (`enum_variant`)
          - `MacroDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroDefinition` (`enum_variant`)
          - `MacroFragmentKind`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroFragmentKind` (`enum_variant`)
          - `MacroInvocation`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroInvocation` (`enum_variant`)
          - `MacroMetavariable`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroMetavariable` (`enum_variant`)
          - `MacroParameter`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroParameter` (`enum_variant`)
          - `MatchArm`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MatchArm` (`enum_variant`)
          - `MatchExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MatchExpression` (`enum_variant`)
          - `MemberExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MemberExpression` (`enum_variant`)
          - `MethodDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::MethodDefinition` (`enum_variant`)
          - `ModuleDeclaration`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ModuleDeclaration` (`enum_variant`)
          - `Node`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Node` (`enum_variant`)
          - `Parameter`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Parameter` (`enum_variant`)
          - `ParameterModifier`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ParameterModifier` (`enum_variant`)
          - `Path`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Path` (`enum_variant`)
          - `PathExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::PathExpression` (`enum_variant`)
          - `PathSegment`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::PathSegment` (`enum_variant`)
          - `Pattern`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Pattern` (`enum_variant`)
          - `PrimitiveType`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::PrimitiveType` (`enum_variant`)
          - `Program`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Program` (`enum_variant`)
          - `RangeExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::RangeExpression` (`enum_variant`)
          - `RegistryBlock`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::RegistryBlock` (`enum_variant`)
          - `RegistryEntry`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::RegistryEntry` (`enum_variant`)
          - `ReturnStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ReturnStatement` (`enum_variant`)
          - `ScopeDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ScopeDefinition` (`enum_variant`)
          - `ScopeHook`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::ScopeHook` (`enum_variant`)
          - `SpawnExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::SpawnExpression` (`enum_variant`)
          - `Statement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Statement` (`enum_variant`)
          - `StructLiteralExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::StructLiteralExpression` (`enum_variant`)
          - `StructLiteralField`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::StructLiteralField` (`enum_variant`)
          - `TestDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestDefinition` (`enum_variant`)
          - `TestMetaSection`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestMetaSection` (`enum_variant`)
          - `TestMetadataEntry`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestMetadataEntry` (`enum_variant`)
          - `TestSkipEntry`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestSkipEntry` (`enum_variant`)
          - `TestSkipSection`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestSkipSection` (`enum_variant`)
          - `TryExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TryExpression` (`enum_variant`)
          - `Type`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Type` (`enum_variant`)
          - `TypeDefinition`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::TypeDefinition` (`enum_variant`)
          - `UnaryExpression`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::UnaryExpression` (`enum_variant`)
          - `UnaryOp`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::UnaryOp` (`enum_variant`)
          - `UseDeclaration`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::UseDeclaration` (`enum_variant`)
          - `Visibility`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::Visibility` (`enum_variant`)
          - `WhileStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::WhileStatement` (`enum_variant`)
          - `WithStatement`
            - `Beskid::Syntax::Nodes::NodeKind::NodeKind::WithStatement` (`enum_variant`)
          - `Beskid::Syntax::Nodes::NodeKind::NodeKind` (`enum`)
        - `Beskid::Syntax::Nodes::NodeKind` (`module`)
      - `NodeList`
        - `Beskid::Syntax::Nodes::NodeList` (`module`)
      - `NodeRef`
        - `NodeRef`
          - `nodeId`
            - `Beskid::Syntax::Nodes::NodeRef::NodeRef::nodeId` (`field`)
          - `syntaxGenerationId`
            - `Beskid::Syntax::Nodes::NodeRef::NodeRef::syntaxGenerationId` (`field`)
          - `Beskid::Syntax::Nodes::NodeRef::NodeRef` (`type`)
        - `Beskid::Syntax::Nodes::NodeRef` (`module`)
      - `NodeSpan`
        - `NodeSpan`
          - `columnEnd`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::columnEnd` (`field`)
          - `columnStart`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::columnStart` (`field`)
          - `end`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::end` (`field`)
          - `lineEnd`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::lineEnd` (`field`)
          - `lineStart`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::lineStart` (`field`)
          - `start`
            - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::start` (`field`)
          - `Beskid::Syntax::Nodes::NodeSpan::NodeSpan` (`type`)
        - `Beskid::Syntax::Nodes::NodeSpan` (`module`)
      - `OptionList`
        - `Beskid::Syntax::Nodes::OptionList` (`module`)
      - `OptionalBlock`
        - `OptionalBlock`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalBlock` (`module`)
      - `OptionalExpression`
        - `OptionalExpression`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalExpression` (`module`)
      - `OptionalIdentifier`
        - `OptionalIdentifier`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalIdentifier` (`module`)
      - `OptionalInjectQualifier`
        - `OptionalInjectQualifier`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalInjectQualifier` (`module`)
      - `OptionalLeadingDocComment`
        - `OptionalLeadingDocComment`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalLeadingDocComment` (`module`)
      - `OptionalParameterModifier`
        - `OptionalParameterModifier`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalParameterModifier` (`module`)
      - `OptionalPath`
        - `OptionalPath`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalPath::OptionalPath` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalPath` (`module`)
      - `OptionalRegistrationLifetime`
        - `OptionalRegistrationLifetime`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalRegistrationLifetime` (`module`)
      - `OptionalTestMetaSection`
        - `OptionalTestMetaSection`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalTestMetaSection` (`module`)
      - `OptionalTestSkipSection`
        - `OptionalTestSkipSection`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalTestSkipSection` (`module`)
      - `OptionalType`
        - `OptionalType`
          - `None`
            - `Beskid::Syntax::Nodes::OptionalType::OptionalType::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::OptionalType::OptionalType::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::OptionalType::OptionalType::payload` (`field`)
          - `Beskid::Syntax::Nodes::OptionalType::OptionalType` (`enum`)
        - `Beskid::Syntax::Nodes::OptionalType` (`module`)
      - `Optionalusize`
        - `Optionalusize`
          - `None`
            - `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::None` (`enum_variant`)
          - `Some`
            - `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::Some` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::payload` (`field`)
          - `Beskid::Syntax::Nodes::Optionalusize::Optionalusize` (`enum`)
        - `Beskid::Syntax::Nodes::Optionalusize` (`module`)
      - `Parameter`
        - `Parameter`
          - `modifier`
            - `Beskid::Syntax::Nodes::Parameter::Parameter::modifier` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::Parameter::Parameter::name` (`field`)
          - `ty`
            - `Beskid::Syntax::Nodes::Parameter::Parameter::ty` (`field`)
          - `Beskid::Syntax::Nodes::Parameter::Parameter` (`type`)
        - `Beskid::Syntax::Nodes::Parameter` (`module`)
      - `ParameterList`
        - `Beskid::Syntax::Nodes::ParameterList` (`module`)
      - `ParameterModifier`
        - `ParameterModifier`
          - `Out`
            - `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier::Out` (`enum_variant`)
          - `Ref`
            - `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier::Ref` (`enum_variant`)
          - `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier` (`enum`)
        - `Beskid::Syntax::Nodes::ParameterModifier` (`module`)
      - `Path`
        - `Path`
          - `segments`
            - `Beskid::Syntax::Nodes::Path::Path::segments` (`field`)
          - `Beskid::Syntax::Nodes::Path::Path` (`type`)
        - `Beskid::Syntax::Nodes::Path` (`module`)
      - `PathExpression`
        - `PathExpression`
          - `path`
            - `Beskid::Syntax::Nodes::PathExpression::PathExpression::path` (`field`)
          - `Beskid::Syntax::Nodes::PathExpression::PathExpression` (`type`)
        - `Beskid::Syntax::Nodes::PathExpression` (`module`)
      - `PathList`
        - `Beskid::Syntax::Nodes::PathList` (`module`)
      - `PathSegment`
        - `PathSegment`
          - `name`
            - `Beskid::Syntax::Nodes::PathSegment::PathSegment::name` (`field`)
          - `typeArgs`
            - `Beskid::Syntax::Nodes::PathSegment::PathSegment::typeArgs` (`field`)
          - `Beskid::Syntax::Nodes::PathSegment::PathSegment` (`type`)
        - `Beskid::Syntax::Nodes::PathSegment` (`module`)
      - `PathSegmentList`
        - `Beskid::Syntax::Nodes::PathSegmentList` (`module`)
      - `Pattern`
        - `Pattern`
          - `Enum`
            - `Beskid::Syntax::Nodes::Pattern::Pattern::Enum` (`enum_variant`)
          - `Identifier`
            - `Beskid::Syntax::Nodes::Pattern::Pattern::Identifier` (`enum_variant`)
          - `Literal`
            - `Beskid::Syntax::Nodes::Pattern::Pattern::Literal` (`enum_variant`)
          - `Wildcard`
            - `Beskid::Syntax::Nodes::Pattern::Pattern::Wildcard` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)
            - `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)
            - `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)
          - `Beskid::Syntax::Nodes::Pattern::Pattern` (`enum`)
        - `Beskid::Syntax::Nodes::Pattern` (`module`)
      - `PatternList`
        - `Beskid::Syntax::Nodes::PatternList` (`module`)
      - `PrimitiveType`
        - `PrimitiveType`
          - `Bool`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Bool` (`enum_variant`)
          - `Char`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Char` (`enum_variant`)
          - `F64`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::F64` (`enum_variant`)
          - `I32`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::I32` (`enum_variant`)
          - `I64`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::I64` (`enum_variant`)
          - `String`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::String` (`enum_variant`)
          - `U8`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::U8` (`enum_variant`)
          - `Unit`
            - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Unit` (`enum_variant`)
          - `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType` (`enum`)
        - `Beskid::Syntax::Nodes::PrimitiveType` (`module`)
      - `Program`
        - `Program`
          - `items`
            - `Beskid::Syntax::Nodes::Program::Program::items` (`field`)
          - `Beskid::Syntax::Nodes::Program::Program` (`type`)
        - `Beskid::Syntax::Nodes::Program` (`module`)
      - `RangeExpression`
        - `RangeExpression`
          - `end`
            - `Beskid::Syntax::Nodes::RangeExpression::RangeExpression::end` (`field`)
          - `start`
            - `Beskid::Syntax::Nodes::RangeExpression::RangeExpression::start` (`field`)
          - `Beskid::Syntax::Nodes::RangeExpression::RangeExpression` (`type`)
        - `Beskid::Syntax::Nodes::RangeExpression` (`module`)
      - `RegistrationLifetime`
        - `RegistrationLifetime`
          - `Single`
            - `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime::Single` (`enum_variant`)
          - `Transient`
            - `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime::Transient` (`enum_variant`)
          - `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime` (`enum`)
        - `Beskid::Syntax::Nodes::RegistrationLifetime` (`module`)
      - `RegistryBlock`
        - `RegistryBlock`
          - `entries`
            - `Beskid::Syntax::Nodes::RegistryBlock::RegistryBlock::entries` (`field`)
          - `Beskid::Syntax::Nodes::RegistryBlock::RegistryBlock` (`type`)
        - `Beskid::Syntax::Nodes::RegistryBlock` (`module`)
      - `RegistryEntry`
        - `RegistryEntry`
          - `implementation`
            - `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry::implementation` (`field`)
          - `target`
            - `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry::target` (`field`)
          - `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry` (`type`)
        - `Beskid::Syntax::Nodes::RegistryEntry` (`module`)
      - `RegistryEntryList`
        - `Beskid::Syntax::Nodes::RegistryEntryList` (`module`)
      - `ReturnStatement`
        - `ReturnStatement`
          - `value`
            - `Beskid::Syntax::Nodes::ReturnStatement::ReturnStatement::value` (`field`)
          - `Beskid::Syntax::Nodes::ReturnStatement::ReturnStatement` (`type`)
        - `Beskid::Syntax::Nodes::ReturnStatement` (`module`)
      - `ScopeDefinition`
        - `ScopeDefinition`
          - `body`
            - `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::body` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::name` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::parameters` (`field`)
          - `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition` (`type`)
        - `Beskid::Syntax::Nodes::ScopeDefinition` (`module`)
      - `ScopeHook`
        - `ScopeHook`
          - `body`
            - `Beskid::Syntax::Nodes::ScopeHook::ScopeHook::body` (`field`)
          - `parameters`
            - `Beskid::Syntax::Nodes::ScopeHook::ScopeHook::parameters` (`field`)
          - `Beskid::Syntax::Nodes::ScopeHook::ScopeHook` (`type`)
        - `Beskid::Syntax::Nodes::ScopeHook` (`module`)
      - `ScopeHookKind`
        - `ScopeHookKind`
          - `Dispose`
            - `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Dispose` (`enum_variant`)
          - `Init`
            - `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Init` (`enum_variant`)
          - `Startup`
            - `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Startup` (`enum_variant`)
          - `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind` (`enum`)
        - `Beskid::Syntax::Nodes::ScopeHookKind` (`module`)
      - `SpawnExpression`
        - `SpawnExpression`
          - `callee`
            - `Beskid::Syntax::Nodes::SpawnExpression::SpawnExpression::callee` (`field`)
          - `Beskid::Syntax::Nodes::SpawnExpression::SpawnExpression` (`type`)
        - `Beskid::Syntax::Nodes::SpawnExpression` (`module`)
      - `Statement`
        - `Statement`
          - `Break`
            - `Beskid::Syntax::Nodes::Statement::Statement::Break` (`enum_variant`)
          - `Continue`
            - `Beskid::Syntax::Nodes::Statement::Statement::Continue` (`enum_variant`)
          - `Expression`
            - `Beskid::Syntax::Nodes::Statement::Statement::Expression` (`enum_variant`)
          - `For`
            - `Beskid::Syntax::Nodes::Statement::Statement::For` (`enum_variant`)
          - `If`
            - `Beskid::Syntax::Nodes::Statement::Statement::If` (`enum_variant`)
          - `Launch`
            - `Beskid::Syntax::Nodes::Statement::Statement::Launch` (`enum_variant`)
          - `Let`
            - `Beskid::Syntax::Nodes::Statement::Statement::Let` (`enum_variant`)
          - `Return`
            - `Beskid::Syntax::Nodes::Statement::Statement::Return` (`enum_variant`)
          - `While`
            - `Beskid::Syntax::Nodes::Statement::Statement::While` (`enum_variant`)
          - `With`
            - `Beskid::Syntax::Nodes::Statement::Statement::With` (`enum_variant`)
          - `payload`
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
            - `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)
          - `Beskid::Syntax::Nodes::Statement::Statement` (`enum`)
        - `Beskid::Syntax::Nodes::Statement` (`module`)
      - `StatementList`
        - `Beskid::Syntax::Nodes::StatementList` (`module`)
      - `StructLiteralExpression`
        - `StructLiteralExpression`
          - `fields`
            - `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression::fields` (`field`)
          - `path`
            - `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression::path` (`field`)
          - `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression` (`type`)
        - `Beskid::Syntax::Nodes::StructLiteralExpression` (`module`)
      - `StructLiteralField`
        - `StructLiteralField`
          - `name`
            - `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField::name` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField::value` (`field`)
          - `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField` (`type`)
        - `Beskid::Syntax::Nodes::StructLiteralField` (`module`)
      - `StructLiteralFieldList`
        - `Beskid::Syntax::Nodes::StructLiteralFieldList` (`module`)
      - `TestDefinition`
        - `TestDefinition`
          - `_meta`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::_meta` (`field`)
          - `_skip`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::_skip` (`field`)
          - `attributes`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::attributes` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::name` (`field`)
          - `statements`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::statements` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::TestDefinition::TestDefinition` (`type`)
        - `Beskid::Syntax::Nodes::TestDefinition` (`module`)
      - `TestMetaSection`
        - `TestMetaSection`
          - `entries`
            - `Beskid::Syntax::Nodes::TestMetaSection::TestMetaSection::entries` (`field`)
          - `Beskid::Syntax::Nodes::TestMetaSection::TestMetaSection` (`type`)
        - `Beskid::Syntax::Nodes::TestMetaSection` (`module`)
      - `TestMetadataEntry`
        - `TestMetadataEntry`
          - `name`
            - `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry::name` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry::value` (`field`)
          - `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry` (`type`)
        - `Beskid::Syntax::Nodes::TestMetadataEntry` (`module`)
      - `TestMetadataEntryList`
        - `Beskid::Syntax::Nodes::TestMetadataEntryList` (`module`)
      - `TestSkipEntry`
        - `TestSkipEntry`
          - `name`
            - `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry::name` (`field`)
          - `value`
            - `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry::value` (`field`)
          - `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry` (`type`)
        - `Beskid::Syntax::Nodes::TestSkipEntry` (`module`)
      - `TestSkipEntryList`
        - `Beskid::Syntax::Nodes::TestSkipEntryList` (`module`)
      - `TestSkipSection`
        - `TestSkipSection`
          - `entries`
            - `Beskid::Syntax::Nodes::TestSkipSection::TestSkipSection::entries` (`field`)
          - `Beskid::Syntax::Nodes::TestSkipSection::TestSkipSection` (`type`)
        - `Beskid::Syntax::Nodes::TestSkipSection` (`module`)
      - `TraversalManifest`
        - `Beskid::Syntax::Nodes::TraversalManifest` (`module`)
      - `TryExpression`
        - `TryExpression`
          - `expr`
            - `Beskid::Syntax::Nodes::TryExpression::TryExpression::expr` (`field`)
          - `Beskid::Syntax::Nodes::TryExpression::TryExpression` (`type`)
        - `Beskid::Syntax::Nodes::TryExpression` (`module`)
      - `Type`
        - `Type`
          - `Array`
            - `Beskid::Syntax::Nodes::Type::Type::Array` (`enum_variant`)
          - `Complex`
            - `Beskid::Syntax::Nodes::Type::Type::Complex` (`enum_variant`)
          - `Function`
            - `Beskid::Syntax::Nodes::Type::Type::Function` (`enum_variant`)
          - `Primitive`
            - `Beskid::Syntax::Nodes::Type::Type::Primitive` (`enum_variant`)
          - `Ref`
            - `Beskid::Syntax::Nodes::Type::Type::Ref` (`enum_variant`)
          - `parameters`
            - `Beskid::Syntax::Nodes::Type::Type::parameters` (`field`)
          - `payload`
            - `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)
            - `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)
            - `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)
            - `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)
          - `returnType`
            - `Beskid::Syntax::Nodes::Type::Type::returnType` (`field`)
          - `Beskid::Syntax::Nodes::Type::Type` (`enum`)
        - `Beskid::Syntax::Nodes::Type` (`module`)
      - `TypeDefinition`
        - `TypeDefinition`
          - `conformances`
            - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::conformances` (`field`)
          - `fields`
            - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::fields` (`field`)
          - `generics`
            - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::generics` (`field`)
          - `name`
            - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::name` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::visibility` (`field`)
          - `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition` (`type`)
        - `Beskid::Syntax::Nodes::TypeDefinition` (`module`)
      - `TypeList`
        - `Beskid::Syntax::Nodes::TypeList` (`module`)
      - `UnaryExpression`
        - `UnaryExpression`
          - `expr`
            - `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression::expr` (`field`)
          - `op`
            - `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression::op` (`field`)
          - `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression` (`type`)
        - `Beskid::Syntax::Nodes::UnaryExpression` (`module`)
      - `UnaryOp`
        - `UnaryOp`
          - `Neg`
            - `Beskid::Syntax::Nodes::UnaryOp::UnaryOp::Neg` (`enum_variant`)
          - `Not`
            - `Beskid::Syntax::Nodes::UnaryOp::UnaryOp::Not` (`enum_variant`)
          - `Beskid::Syntax::Nodes::UnaryOp::UnaryOp` (`enum`)
        - `Beskid::Syntax::Nodes::UnaryOp` (`module`)
      - `UseDeclaration`
        - `UseDeclaration`
          - `alias`
            - `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::alias` (`field`)
          - `path`
            - `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::path` (`field`)
          - `visibility`
            - `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::visibility` (`field`)
          - `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration` (`type`)
        - `Beskid::Syntax::Nodes::UseDeclaration` (`module`)
      - `Visibility`
        - `Visibility`
          - `Private`
            - `Beskid::Syntax::Nodes::Visibility::Visibility::Private` (`enum_variant`)
          - `Public`
            - `Beskid::Syntax::Nodes::Visibility::Visibility::Public` (`enum_variant`)
          - `Beskid::Syntax::Nodes::Visibility::Visibility` (`enum`)
        - `Beskid::Syntax::Nodes::Visibility` (`module`)
      - `Visit`
        - `SyntaxVisitor`
          - `Enter`
            - `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::Enter` (`contract_method`)
          - `Exit`
            - `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::Exit` (`contract_method`)
          - `node`
            - `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::node` (`parameter`)
            - `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::node` (`parameter`)
          - `Beskid::Syntax::Nodes::Visit::SyntaxVisitor` (`contract`)
        - `Beskid::Syntax::Nodes::Visit` (`module`)
      - `WhileStatement`
        - `WhileStatement`
          - `body`
            - `Beskid::Syntax::Nodes::WhileStatement::WhileStatement::body` (`field`)
          - `condition`
            - `Beskid::Syntax::Nodes::WhileStatement::WhileStatement::condition` (`field`)
          - `Beskid::Syntax::Nodes::WhileStatement::WhileStatement` (`type`)
        - `Beskid::Syntax::Nodes::WhileStatement` (`module`)
      - `WithStatement`
        - `WithStatement`
          - `arguments`
            - `Beskid::Syntax::Nodes::WithStatement::WithStatement::arguments` (`field`)
          - `body`
            - `Beskid::Syntax::Nodes::WithStatement::WithStatement::body` (`field`)
          - `scopeName`
            - `Beskid::Syntax::Nodes::WithStatement::WithStatement::scopeName` (`field`)
          - `Beskid::Syntax::Nodes::WithStatement::WithStatement` (`type`)
        - `Beskid::Syntax::Nodes::WithStatement` (`module`)
      - `Beskid::Syntax::Nodes` (`module`)
    - `SyntaxFacadeVersion`
      - `Beskid::Syntax::SyntaxFacadeVersion` (`function`)
    - `Beskid::Syntax` (`module`)
- `Collections`
  - `Array`
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
- `Concurrency`
  - `Channel`
    - `Channel`
      - `handle`
        - `Concurrency::Channel::Channel::handle` (`field`)
      - `Concurrency::Channel::Channel` (`type`)
    - `Close`
      - `self`
        - `Concurrency::Channel::Close::self` (`parameter`)
      - `Concurrency::Channel::Close` (`function`)
    - `Create`
      - `Concurrency::Channel::Create` (`function`)
    - `CreateWithOptions`
      - `options`
        - `Concurrency::Channel::CreateWithOptions::options` (`parameter`)
      - `Concurrency::Channel::CreateWithOptions` (`function`)
    - `Handle`
      - `self`
        - `Concurrency::Channel::Handle::self` (`parameter`)
      - `Concurrency::Channel::Handle` (`function`)
    - `MapReceiveError`
      - `status`
        - `Concurrency::Channel::MapReceiveError::status` (`parameter`)
      - `Concurrency::Channel::MapReceiveError` (`function`)
    - `MapSendStatus`
      - `status`
        - `Concurrency::Channel::MapSendStatus::status` (`parameter`)
      - `Concurrency::Channel::MapSendStatus` (`function`)
    - `Receive`
      - `self`
        - `Concurrency::Channel::Receive::self` (`parameter`)
      - `Concurrency::Channel::Receive` (`function`)
    - `Send`
      - `self`
        - `Concurrency::Channel::Send::self` (`parameter`)
      - `value`
        - `Concurrency::Channel::Send::value` (`parameter`)
      - `Concurrency::Channel::Send` (`function`)
    - `SendOk`
      - `Concurrency::Channel::SendOk` (`type`)
    - `TryReceive`
      - `self`
        - `Concurrency::Channel::TryReceive::self` (`parameter`)
      - `Concurrency::Channel::TryReceive` (`function`)
    - `TrySend`
      - `self`
        - `Concurrency::Channel::TrySend::self` (`parameter`)
      - `value`
        - `Concurrency::Channel::TrySend::value` (`parameter`)
      - `Concurrency::Channel::TrySend` (`function`)
    - `Concurrency::Channel` (`module`)
  - `ChannelError`
    - `ChannelError`
      - `Cancelled`
        - `Concurrency::ChannelError::ChannelError::Cancelled` (`enum_variant`)
      - `Closed`
        - `Concurrency::ChannelError::ChannelError::Closed` (`enum_variant`)
      - `Concurrency::ChannelError::ChannelError` (`enum`)
    - `Concurrency::ChannelError` (`module`)
  - `ChannelOptions`
    - `Bounded`
      - `capacity`
        - `Concurrency::ChannelOptions::Bounded::capacity` (`parameter`)
      - `Concurrency::ChannelOptions::Bounded` (`function`)
    - `ChannelCapacity`
      - `Bounded`
        - `Concurrency::ChannelOptions::ChannelCapacity::Bounded` (`enum_variant`)
      - `Unbounded`
        - `Concurrency::ChannelOptions::ChannelCapacity::Unbounded` (`enum_variant`)
      - `capacity`
        - `Concurrency::ChannelOptions::ChannelCapacity::capacity` (`field`)
      - `Concurrency::ChannelOptions::ChannelCapacity` (`enum`)
    - `ChannelOptions`
      - `capacity`
        - `Concurrency::ChannelOptions::ChannelOptions::capacity` (`field`)
      - `singleReader`
        - `Concurrency::ChannelOptions::ChannelOptions::singleReader` (`field`)
      - `singleWriter`
        - `Concurrency::ChannelOptions::ChannelOptions::singleWriter` (`field`)
      - `Concurrency::ChannelOptions::ChannelOptions` (`type`)
    - `Default`
      - `Concurrency::ChannelOptions::Default` (`function`)
    - `EncodeCapacity`
      - `options`
        - `Concurrency::ChannelOptions::EncodeCapacity::options` (`parameter`)
      - `Concurrency::ChannelOptions::EncodeCapacity` (`function`)
    - `EncodeFlags`
      - `options`
        - `Concurrency::ChannelOptions::EncodeFlags::options` (`parameter`)
      - `Concurrency::ChannelOptions::EncodeFlags` (`function`)
    - `SingleReader`
      - `Concurrency::ChannelOptions::SingleReader` (`function`)
    - `SingleWriter`
      - `Concurrency::ChannelOptions::SingleWriter` (`function`)
    - `Unbounded`
      - `Concurrency::ChannelOptions::Unbounded` (`function`)
    - `Concurrency::ChannelOptions` (`module`)
  - `Fiber`
    - `Cancel`
      - `self`
        - `Concurrency::Fiber::Cancel::self` (`parameter`)
      - `Concurrency::Fiber::Cancel` (`function`)
    - `Detach`
      - `self`
        - `Concurrency::Fiber::Detach::self` (`parameter`)
      - `Concurrency::Fiber::Detach` (`function`)
    - `Fiber`
      - `OnCancelled`
        - `Concurrency::Fiber::Fiber::OnCancelled` (`field`)
      - `handle`
        - `Concurrency::Fiber::Fiber::handle` (`field`)
      - `Concurrency::Fiber::Fiber` (`type`)
    - `Join`
      - `self`
        - `Concurrency::Fiber::Join::self` (`parameter`)
      - `Concurrency::Fiber::Join` (`function`)
    - `MapJoinError`
      - `status`
        - `Concurrency::Fiber::MapJoinError::status` (`parameter`)
      - `Concurrency::Fiber::MapJoinError` (`function`)
    - `Concurrency::Fiber` (`module`)
  - `FiberError`
    - `FiberError`
      - `Cancelled`
        - `Concurrency::FiberError::FiberError::Cancelled` (`enum_variant`)
      - `Panicked`
        - `Concurrency::FiberError::FiberError::Panicked` (`enum_variant`)
      - `StackOverflow`
        - `Concurrency::FiberError::FiberError::StackOverflow` (`enum_variant`)
      - `code`
        - `Concurrency::FiberError::FiberError::code` (`field`)
      - `Concurrency::FiberError::FiberError` (`enum`)
    - `Concurrency::FiberError` (`module`)
  - `FiberJoinStatus`
    - `Cancelled`
      - `Concurrency::FiberJoinStatus::Cancelled` (`function`)
    - `NotDone`
      - `Concurrency::FiberJoinStatus::NotDone` (`function`)
    - `Ok`
      - `Concurrency::FiberJoinStatus::Ok` (`function`)
    - `Panicked`
      - `Concurrency::FiberJoinStatus::Panicked` (`function`)
    - `StackOverflow`
      - `Concurrency::FiberJoinStatus::StackOverflow` (`function`)
    - `Concurrency::FiberJoinStatus` (`module`)
  - `Hub`
    - `Create`
      - `Concurrency::Hub::Create` (`function`)
    - `Hub`
      - `handle`
        - `Concurrency::Hub::Hub::handle` (`field`)
      - `Concurrency::Hub::Hub` (`type`)
    - `Register`
      - `channel`
        - `Concurrency::Hub::Register::channel` (`parameter`)
      - `index`
        - `Concurrency::Hub::Register::index` (`parameter`)
      - `self`
        - `Concurrency::Hub::Register::self` (`parameter`)
      - `Concurrency::Hub::Register` (`function`)
    - `Unregister`
      - `index`
        - `Concurrency::Hub::Unregister::index` (`parameter`)
      - `self`
        - `Concurrency::Hub::Unregister::self` (`parameter`)
      - `Concurrency::Hub::Unregister` (`function`)
    - `WaitReceive`
      - `self`
        - `Concurrency::Hub::WaitReceive::self` (`parameter`)
      - `Concurrency::Hub::WaitReceive` (`function`)
    - `Concurrency::Hub` (`module`)
  - `HubError`
    - `HubError`
      - `Cancelled`
        - `Concurrency::HubError::HubError::Cancelled` (`enum_variant`)
      - `Closed`
        - `Concurrency::HubError::HubError::Closed` (`enum_variant`)
      - `Limit`
        - `Concurrency::HubError::HubError::Limit` (`enum_variant`)
      - `Concurrency::HubError::HubError` (`enum`)
    - `Concurrency::HubError` (`module`)
  - `HubReceiveResult`
    - `HubReceiveResult`
      - `index`
        - `Concurrency::HubReceiveResult::HubReceiveResult::index` (`field`)
      - `value`
        - `Concurrency::HubReceiveResult::HubReceiveResult::value` (`field`)
      - `Concurrency::HubReceiveResult::HubReceiveResult` (`type`)
    - `Concurrency::HubReceiveResult` (`module`)
  - `Mutex`
    - `Create`
      - `Concurrency::Mutex::Create` (`function`)
    - `Lock`
      - `self`
        - `Concurrency::Mutex::Lock::self` (`parameter`)
      - `Concurrency::Mutex::Lock` (`function`)
    - `Mutex`
      - `handle`
        - `Concurrency::Mutex::Mutex::handle` (`field`)
      - `Concurrency::Mutex::Mutex` (`type`)
    - `TryLock`
      - `self`
        - `Concurrency::Mutex::TryLock::self` (`parameter`)
      - `Concurrency::Mutex::TryLock` (`function`)
    - `Unlock`
      - `guard`
        - `Concurrency::Mutex::Unlock::guard` (`parameter`)
      - `Concurrency::Mutex::Unlock` (`function`)
    - `Concurrency::Mutex` (`module`)
  - `MutexError`
    - `MutexError`
      - `Cancelled`
        - `Concurrency::MutexError::MutexError::Cancelled` (`enum_variant`)
      - `Concurrency::MutexError::MutexError` (`enum`)
    - `Concurrency::MutexError` (`module`)
  - `MutexGuard`
    - `MutexGuard`
      - `mutexHandle`
        - `Concurrency::MutexGuard::MutexGuard::mutexHandle` (`field`)
      - `Concurrency::MutexGuard::MutexGuard` (`type`)
    - `Concurrency::MutexGuard` (`module`)
  - `NowMillis`
    - `Concurrency::NowMillis` (`function`)
  - `ProcessorCount`
    - `Concurrency::ProcessorCount` (`function`)
  - `SetProcessorCount`
    - `count`
      - `Concurrency::SetProcessorCount::count` (`parameter`)
    - `Concurrency::SetProcessorCount` (`function`)
  - `Status`
    - `Cancelled`
      - `Concurrency::Status::Cancelled` (`function`)
    - `Closed`
      - `Concurrency::Status::Closed` (`function`)
    - `HubEmpty`
      - `Concurrency::Status::HubEmpty` (`function`)
    - `HubLimit`
      - `Concurrency::Status::HubLimit` (`function`)
    - `HubNotFound`
      - `Concurrency::Status::HubNotFound` (`function`)
    - `MutexBusy`
      - `Concurrency::Status::MutexBusy` (`function`)
    - `Ok`
      - `Concurrency::Status::Ok` (`function`)
    - `WouldBlock`
      - `Concurrency::Status::WouldBlock` (`function`)
  - `WaitGroup`
    - `Add`
      - `delta`
        - `Concurrency::WaitGroup::Add::delta` (`parameter`)
      - `self`
        - `Concurrency::WaitGroup::Add::self` (`parameter`)
      - `Concurrency::WaitGroup::Add` (`function`)
    - `Create`
      - `Concurrency::WaitGroup::Create` (`function`)
    - `Done`
      - `self`
        - `Concurrency::WaitGroup::Done::self` (`parameter`)
      - `Concurrency::WaitGroup::Done` (`function`)
    - `Wait`
      - `self`
        - `Concurrency::WaitGroup::Wait::self` (`parameter`)
      - `Concurrency::WaitGroup::Wait` (`function`)
    - `WaitGroup`
      - `handle`
        - `Concurrency::WaitGroup::WaitGroup::handle` (`field`)
      - `Concurrency::WaitGroup::WaitGroup` (`type`)
    - `Concurrency::WaitGroup` (`module`)
  - `Yield`
    - `Concurrency::Yield` (`function`)
  - `Concurrency` (`module`)
- `Console`
  - `Capabilities`
    - `Capabilities`
      - `colorDisabled`
        - `Console::Capabilities::Capabilities::colorDisabled` (`field`)
      - `colorForced`
        - `Console::Capabilities::Capabilities::colorForced` (`field`)
      - `isTty`
        - `Console::Capabilities::Capabilities::isTty` (`field`)
      - `model`
        - `Console::Capabilities::Capabilities::model` (`field`)
      - `Console::Capabilities::Capabilities` (`type`)
    - `ColorModel`
      - `Basic16`
        - `Console::Capabilities::ColorModel::Basic16` (`enum_variant`)
      - `Basic8`
        - `Console::Capabilities::ColorModel::Basic8` (`enum_variant`)
      - `Indexed256`
        - `Console::Capabilities::ColorModel::Indexed256` (`enum_variant`)
      - `TrueColor`
        - `Console::Capabilities::ColorModel::TrueColor` (`enum_variant`)
      - `Console::Capabilities::ColorModel` (`enum`)
    - `EffectiveColorModel`
      - `caps`
        - `Console::Capabilities::EffectiveColorModel::caps` (`parameter`)
      - `Console::Capabilities::EffectiveColorModel` (`function`)
    - `IsStreamTty`
      - `fd`
        - `Console::Capabilities::IsStreamTty::fd` (`parameter`)
      - `Console::Capabilities::IsStreamTty` (`function`)
    - `ProbeStdout`
      - `Console::Capabilities::ProbeStdout` (`function`)
    - `ShouldEmitAnsi`
      - `Console::Capabilities::ShouldEmitAnsi` (`function`)
    - `ShouldStripColor`
      - `caps`
        - `Console::Capabilities::ShouldStripColor::caps` (`parameter`)
      - `Console::Capabilities::ShouldStripColor` (`function`)
    - `Console::Capabilities` (`module`)
  - `ConsoleMessage`
    - `ConsoleMessage`
      - `Resize`
        - `Console::ConsoleMessage::ConsoleMessage::Resize` (`enum_variant`)
      - `Tick`
        - `Console::ConsoleMessage::ConsoleMessage::Tick` (`enum_variant`)
      - `size`
        - `Console::ConsoleMessage::ConsoleMessage::size` (`field`)
      - `Console::ConsoleMessage::ConsoleMessage` (`enum`)
    - `Console::ConsoleMessage` (`module`)
  - `ConsoleSize`
    - `columns`
      - `Console::ConsoleSize::columns` (`field`)
    - `rows`
      - `Console::ConsoleSize::rows` (`field`)
    - `Console::ConsoleSize` (`type`)
  - `Controls`
    - `Contracts`
      - `ConsoleControl`
        - `Measure`
          - `Console::Controls::Contracts::ConsoleControl::Measure` (`contract_method`)
        - `Render`
          - `Console::Controls::Contracts::ConsoleControl::Render` (`contract_method`)
        - `available`
          - `Console::Controls::Contracts::ConsoleControl::available` (`parameter`)
        - `size`
          - `Console::Controls::Contracts::ConsoleControl::size` (`parameter`)
        - `Console::Controls::Contracts::ConsoleControl` (`contract`)
      - `Container`
        - `ChildCount`
          - `Console::Controls::Contracts::Container::ChildCount` (`contract_method`)
        - `Console::Controls::Contracts::Container` (`contract`)
      - `FramedControl`
        - `UseUnicodeFrame`
          - `Console::Controls::Contracts::FramedControl::UseUnicodeFrame` (`contract_method`)
        - `Console::Controls::Contracts::FramedControl` (`contract`)
      - `LiveControl`
        - `OnTick`
          - `Console::Controls::Contracts::LiveControl::OnTick` (`contract_method`)
        - `Console::Controls::Contracts::LiveControl` (`contract`)
      - `MarginProvider`
        - `Margin`
          - `Console::Controls::Contracts::MarginProvider::Margin` (`contract_method`)
        - `Console::Controls::Contracts::MarginProvider` (`contract`)
      - `PaddingProvider`
        - `Padding`
          - `Console::Controls::Contracts::PaddingProvider::Padding` (`contract_method`)
        - `Console::Controls::Contracts::PaddingProvider` (`contract`)
      - `Console::Controls::Contracts` (`module`)
    - `Frame`
      - `Ascii`
        - `Console::Controls::Frame::Ascii` (`function`)
      - `ForHost`
        - `useUnicode`
          - `Console::Controls::Frame::ForHost::useUnicode` (`parameter`)
        - `Console::Controls::Frame::ForHost` (`function`)
      - `FrameChars`
        - `bottom`
          - `Console::Controls::Frame::FrameChars::bottom` (`field`)
        - `bottomLeft`
          - `Console::Controls::Frame::FrameChars::bottomLeft` (`field`)
        - `bottomRight`
          - `Console::Controls::Frame::FrameChars::bottomRight` (`field`)
        - `side`
          - `Console::Controls::Frame::FrameChars::side` (`field`)
        - `top`
          - `Console::Controls::Frame::FrameChars::top` (`field`)
        - `topLeft`
          - `Console::Controls::Frame::FrameChars::topLeft` (`field`)
        - `topRight`
          - `Console::Controls::Frame::FrameChars::topRight` (`field`)
        - `Console::Controls::Frame::FrameChars` (`type`)
      - `PreferUnicodeFrame`
        - `Console::Controls::Frame::PreferUnicodeFrame` (`function`)
      - `Repeat`
        - `count`
          - `Console::Controls::Frame::Repeat::count` (`parameter`)
        - `unit`
          - `Console::Controls::Frame::Repeat::unit` (`parameter`)
        - `Console::Controls::Frame::Repeat` (`function`)
      - `Unicode`
        - `Console::Controls::Frame::Unicode` (`function`)
      - `Console::Controls::Frame` (`module`)
    - `HorizontalStack`
      - `ChildCount`
        - `stack`
          - `Console::Controls::HorizontalStack::ChildCount::stack` (`parameter`)
        - `Console::Controls::HorizontalStack::ChildCount` (`function`)
      - `HorizontalStack`
        - `childCount`
          - `Console::Controls::HorizontalStack::HorizontalStack::childCount` (`field`)
        - `segment`
          - `Console::Controls::HorizontalStack::HorizontalStack::segment` (`field`)
        - `Console::Controls::HorizontalStack::HorizontalStack` (`type`)
      - `Measure`
        - `available`
          - `Console::Controls::HorizontalStack::Measure::available` (`parameter`)
        - `stack`
          - `Console::Controls::HorizontalStack::Measure::stack` (`parameter`)
        - `Console::Controls::HorizontalStack::Measure` (`function`)
      - `New`
        - `Console::Controls::HorizontalStack::New` (`function`)
      - `Render`
        - `size`
          - `Console::Controls::HorizontalStack::Render::size` (`parameter`)
        - `stack`
          - `Console::Controls::HorizontalStack::Render::stack` (`parameter`)
        - `Console::Controls::HorizontalStack::Render` (`function`)
      - `RenderWithContext`
        - `ctx`
          - `Console::Controls::HorizontalStack::RenderWithContext::ctx` (`parameter`)
        - `size`
          - `Console::Controls::HorizontalStack::RenderWithContext::size` (`parameter`)
        - `stack`
          - `Console::Controls::HorizontalStack::RenderWithContext::stack` (`parameter`)
        - `Console::Controls::HorizontalStack::RenderWithContext` (`function`)
      - `WithChild`
        - `body`
          - `Console::Controls::HorizontalStack::WithChild::body` (`parameter`)
        - `stack`
          - `Console::Controls::HorizontalStack::WithChild::stack` (`parameter`)
        - `Console::Controls::HorizontalStack::WithChild` (`function`)
      - `Console::Controls::HorizontalStack` (`module`)
    - `LiveTick`
      - `LiveTickState`
        - `hasProgressBar`
          - `Console::Controls::LiveTick::LiveTickState::hasProgressBar` (`field`)
        - `progressBar`
          - `Console::Controls::LiveTick::LiveTickState::progressBar` (`field`)
        - `Console::Controls::LiveTick::LiveTickState` (`type`)
      - `New`
        - `Console::Controls::LiveTick::New` (`function`)
      - `Pulse`
        - `state`
          - `Console::Controls::LiveTick::Pulse::state` (`parameter`)
        - `Console::Controls::LiveTick::Pulse` (`function`)
      - `RegisterProgressBar`
        - `bar`
          - `Console::Controls::LiveTick::RegisterProgressBar::bar` (`parameter`)
        - `state`
          - `Console::Controls::LiveTick::RegisterProgressBar::state` (`parameter`)
        - `Console::Controls::LiveTick::RegisterProgressBar` (`function`)
      - `Console::Controls::LiveTick` (`module`)
    - `Panel`
      - `BodyLine`
        - `frame`
          - `Console::Controls::Panel::BodyLine::frame` (`parameter`)
        - `innerWidth`
          - `Console::Controls::Panel::BodyLine::innerWidth` (`parameter`)
        - `panel`
          - `Console::Controls::Panel::BodyLine::panel` (`parameter`)
        - `Console::Controls::Panel::BodyLine` (`function`)
      - `ContentWidth`
        - `panel`
          - `Console::Controls::Panel::ContentWidth::panel` (`parameter`)
        - `Console::Controls::Panel::ContentWidth` (`function`)
      - `Margin`
        - `panel`
          - `Console::Controls::Panel::Margin::panel` (`parameter`)
        - `Console::Controls::Panel::Margin` (`function`)
      - `Measure`
        - `available`
          - `Console::Controls::Panel::Measure::available` (`parameter`)
        - `panel`
          - `Console::Controls::Panel::Measure::panel` (`parameter`)
        - `Console::Controls::Panel::Measure` (`function`)
      - `New`
        - `body`
          - `Console::Controls::Panel::New::body` (`parameter`)
        - `Console::Controls::Panel::New` (`function`)
      - `Padding`
        - `panel`
          - `Console::Controls::Panel::Padding::panel` (`parameter`)
        - `Console::Controls::Panel::Padding` (`function`)
      - `Panel`
        - `align`
          - `Console::Controls::Panel::Panel::align` (`field`)
        - `body`
          - `Console::Controls::Panel::Panel::body` (`field`)
        - `forceAscii`
          - `Console::Controls::Panel::Panel::forceAscii` (`field`)
        - `margin`
          - `Console::Controls::Panel::Panel::margin` (`field`)
        - `padding`
          - `Console::Controls::Panel::Panel::padding` (`field`)
        - `title`
          - `Console::Controls::Panel::Panel::title` (`field`)
        - `Console::Controls::Panel::Panel` (`type`)
      - `Render`
        - `panel`
          - `Console::Controls::Panel::Render::panel` (`parameter`)
        - `size`
          - `Console::Controls::Panel::Render::size` (`parameter`)
        - `Console::Controls::Panel::Render` (`function`)
      - `RenderWithContext`
        - `ctx`
          - `Console::Controls::Panel::RenderWithContext::ctx` (`parameter`)
        - `panel`
          - `Console::Controls::Panel::RenderWithContext::panel` (`parameter`)
        - `size`
          - `Console::Controls::Panel::RenderWithContext::size` (`parameter`)
        - `Console::Controls::Panel::RenderWithContext` (`function`)
      - `TopBorder`
        - `frame`
          - `Console::Controls::Panel::TopBorder::frame` (`parameter`)
        - `innerWidth`
          - `Console::Controls::Panel::TopBorder::innerWidth` (`parameter`)
        - `panel`
          - `Console::Controls::Panel::TopBorder::panel` (`parameter`)
        - `Console::Controls::Panel::TopBorder` (`function`)
      - `UseUnicodeFrame`
        - `panel`
          - `Console::Controls::Panel::UseUnicodeFrame::panel` (`parameter`)
        - `Console::Controls::Panel::UseUnicodeFrame` (`function`)
      - `WithAlignment`
        - `align`
          - `Console::Controls::Panel::WithAlignment::align` (`parameter`)
        - `panel`
          - `Console::Controls::Panel::WithAlignment::panel` (`parameter`)
        - `Console::Controls::Panel::WithAlignment` (`function`)
      - `WithAsciiFrame`
        - `panel`
          - `Console::Controls::Panel::WithAsciiFrame::panel` (`parameter`)
        - `Console::Controls::Panel::WithAsciiFrame` (`function`)
      - `WithPadding`
        - `padding`
          - `Console::Controls::Panel::WithPadding::padding` (`parameter`)
        - `panel`
          - `Console::Controls::Panel::WithPadding::panel` (`parameter`)
        - `Console::Controls::Panel::WithPadding` (`function`)
      - `WithTitle`
        - `panel`
          - `Console::Controls::Panel::WithTitle::panel` (`parameter`)
        - `title`
          - `Console::Controls::Panel::WithTitle::title` (`parameter`)
        - `Console::Controls::Panel::WithTitle` (`function`)
      - `Console::Controls::Panel` (`module`)
    - `ProgressBar`
      - `BarBody`
        - `bar`
          - `Console::Controls::ProgressBar::BarBody::bar` (`parameter`)
        - `width`
          - `Console::Controls::ProgressBar::BarBody::width` (`parameter`)
        - `Console::Controls::ProgressBar::BarBody` (`function`)
      - `Measure`
        - `available`
          - `Console::Controls::ProgressBar::Measure::available` (`parameter`)
        - `bar`
          - `Console::Controls::ProgressBar::Measure::bar` (`parameter`)
        - `Console::Controls::ProgressBar::Measure` (`function`)
      - `New`
        - `Console::Controls::ProgressBar::New` (`function`)
      - `OnTick`
        - `bar`
          - `Console::Controls::ProgressBar::OnTick::bar` (`parameter`)
        - `Console::Controls::ProgressBar::OnTick` (`function`)
      - `ProgressBar`
        - `anchorCol`
          - `Console::Controls::ProgressBar::ProgressBar::anchorCol` (`field`)
        - `anchorRow`
          - `Console::Controls::ProgressBar::ProgressBar::anchorRow` (`field`)
        - `onTick`
          - `Console::Controls::ProgressBar::ProgressBar::onTick` (`field`)
        - `percent`
          - `Console::Controls::ProgressBar::ProgressBar::percent` (`field`)
        - `Console::Controls::ProgressBar::ProgressBar` (`type`)
      - `Render`
        - `bar`
          - `Console::Controls::ProgressBar::Render::bar` (`parameter`)
        - `size`
          - `Console::Controls::ProgressBar::Render::size` (`parameter`)
        - `Console::Controls::ProgressBar::Render` (`function`)
      - `RenderIncremental`
        - `bar`
          - `Console::Controls::ProgressBar::RenderIncremental::bar` (`parameter`)
        - `size`
          - `Console::Controls::ProgressBar::RenderIncremental::size` (`parameter`)
        - `Console::Controls::ProgressBar::RenderIncremental` (`function`)
      - `Tick`
        - `bar`
          - `Console::Controls::ProgressBar::Tick::bar` (`parameter`)
        - `Console::Controls::ProgressBar::Tick` (`function`)
      - `WithAnchor`
        - `bar`
          - `Console::Controls::ProgressBar::WithAnchor::bar` (`parameter`)
        - `col`
          - `Console::Controls::ProgressBar::WithAnchor::col` (`parameter`)
        - `row`
          - `Console::Controls::ProgressBar::WithAnchor::row` (`parameter`)
        - `Console::Controls::ProgressBar::WithAnchor` (`function`)
      - `WithPercent`
        - `bar`
          - `Console::Controls::ProgressBar::WithPercent::bar` (`parameter`)
        - `percent`
          - `Console::Controls::ProgressBar::WithPercent::percent` (`parameter`)
        - `Console::Controls::ProgressBar::WithPercent` (`function`)
      - `Console::Controls::ProgressBar` (`module`)
    - `RenderContext`
      - `AdvanceRow`
        - `ctx`
          - `Console::Controls::RenderContext::AdvanceRow::ctx` (`parameter`)
        - `Console::Controls::RenderContext::AdvanceRow` (`function`)
      - `EraseLineAndRender`
        - `ctx`
          - `Console::Controls::RenderContext::EraseLineAndRender::ctx` (`parameter`)
        - `line`
          - `Console::Controls::RenderContext::EraseLineAndRender::line` (`parameter`)
        - `Console::Controls::RenderContext::EraseLineAndRender` (`function`)
      - `MoveTo`
        - `col`
          - `Console::Controls::RenderContext::MoveTo::col` (`parameter`)
        - `ctx`
          - `Console::Controls::RenderContext::MoveTo::ctx` (`parameter`)
        - `row`
          - `Console::Controls::RenderContext::MoveTo::row` (`parameter`)
        - `Console::Controls::RenderContext::MoveTo` (`function`)
      - `New`
        - `col`
          - `Console::Controls::RenderContext::New::col` (`parameter`)
        - `row`
          - `Console::Controls::RenderContext::New::row` (`parameter`)
        - `Console::Controls::RenderContext::New` (`function`)
      - `RenderAt`
        - `ctx`
          - `Console::Controls::RenderContext::RenderAt::ctx` (`parameter`)
        - `text`
          - `Console::Controls::RenderContext::RenderAt::text` (`parameter`)
        - `Console::Controls::RenderContext::RenderAt` (`function`)
      - `RenderContext`
        - `cursorCol`
          - `Console::Controls::RenderContext::RenderContext::cursorCol` (`field`)
        - `cursorRow`
          - `Console::Controls::RenderContext::RenderContext::cursorRow` (`field`)
        - `incremental`
          - `Console::Controls::RenderContext::RenderContext::incremental` (`field`)
        - `originCol`
          - `Console::Controls::RenderContext::RenderContext::originCol` (`field`)
        - `originRow`
          - `Console::Controls::RenderContext::RenderContext::originRow` (`field`)
        - `Console::Controls::RenderContext::RenderContext` (`type`)
      - `WithoutIncremental`
        - `col`
          - `Console::Controls::RenderContext::WithoutIncremental::col` (`parameter`)
        - `row`
          - `Console::Controls::RenderContext::WithoutIncremental::row` (`parameter`)
        - `Console::Controls::RenderContext::WithoutIncremental` (`function`)
      - `Console::Controls::RenderContext` (`module`)
    - `VerticalStack`
      - `ChildCount`
        - `stack`
          - `Console::Controls::VerticalStack::ChildCount::stack` (`parameter`)
        - `Console::Controls::VerticalStack::ChildCount` (`function`)
      - `Measure`
        - `available`
          - `Console::Controls::VerticalStack::Measure::available` (`parameter`)
        - `stack`
          - `Console::Controls::VerticalStack::Measure::stack` (`parameter`)
        - `Console::Controls::VerticalStack::Measure` (`function`)
      - `New`
        - `Console::Controls::VerticalStack::New` (`function`)
      - `Render`
        - `size`
          - `Console::Controls::VerticalStack::Render::size` (`parameter`)
        - `stack`
          - `Console::Controls::VerticalStack::Render::stack` (`parameter`)
        - `Console::Controls::VerticalStack::Render` (`function`)
      - `RenderWithContext`
        - `ctx`
          - `Console::Controls::VerticalStack::RenderWithContext::ctx` (`parameter`)
        - `size`
          - `Console::Controls::VerticalStack::RenderWithContext::size` (`parameter`)
        - `stack`
          - `Console::Controls::VerticalStack::RenderWithContext::stack` (`parameter`)
        - `Console::Controls::VerticalStack::RenderWithContext` (`function`)
      - `VerticalStack`
        - `childBodies`
          - `Console::Controls::VerticalStack::VerticalStack::childBodies` (`field`)
        - `childCount`
          - `Console::Controls::VerticalStack::VerticalStack::childCount` (`field`)
        - `Console::Controls::VerticalStack::VerticalStack` (`type`)
      - `WithChild`
        - `body`
          - `Console::Controls::VerticalStack::WithChild::body` (`parameter`)
        - `stack`
          - `Console::Controls::VerticalStack::WithChild::stack` (`parameter`)
        - `Console::Controls::VerticalStack::WithChild` (`function`)
      - `Console::Controls::VerticalStack` (`module`)
  - `Format`
    - `Attributes`
      - `ApplyAttrList`
        - `attrs`
          - `Console::Format::Attributes::ApplyAttrList::attrs` (`parameter`)
        - `chain`
          - `Console::Format::Attributes::ApplyAttrList::chain` (`parameter`)
        - `Console::Format::Attributes::ApplyAttrList` (`function`)
      - `ApplyAttrToken`
        - `chain`
          - `Console::Format::Attributes::ApplyAttrToken::chain` (`parameter`)
        - `token`
          - `Console::Format::Attributes::ApplyAttrToken::token` (`parameter`)
        - `Console::Format::Attributes::ApplyAttrToken` (`function`)
      - `ParseColor`
        - `value`
          - `Console::Format::Attributes::ParseColor::value` (`parameter`)
        - `Console::Format::Attributes::ParseColor` (`function`)
      - `ParseDecimalDigit`
        - `c`
          - `Console::Format::Attributes::ParseDecimalDigit::c` (`parameter`)
        - `Console::Format::Attributes::ParseDecimalDigit` (`function`)
      - `ParseHexByte`
        - `two`
          - `Console::Format::Attributes::ParseHexByte::two` (`parameter`)
        - `Console::Format::Attributes::ParseHexByte` (`function`)
      - `ParseHexColor`
        - `hex`
          - `Console::Format::Attributes::ParseHexColor::hex` (`parameter`)
        - `Console::Format::Attributes::ParseHexColor` (`function`)
      - `ParseHexNibble`
        - `digit`
          - `Console::Format::Attributes::ParseHexNibble::digit` (`parameter`)
        - `Console::Format::Attributes::ParseHexNibble` (`function`)
      - `ParseNamedColor`
        - `name`
          - `Console::Format::Attributes::ParseNamedColor::name` (`parameter`)
        - `Console::Format::Attributes::ParseNamedColor` (`function`)
      - `ParseRgbTriplet`
        - `value`
          - `Console::Format::Attributes::ParseRgbTriplet::value` (`parameter`)
        - `Console::Format::Attributes::ParseRgbTriplet` (`function`)
      - `ParseU8`
        - `digits`
          - `Console::Format::Attributes::ParseU8::digits` (`parameter`)
        - `Console::Format::Attributes::ParseU8` (`function`)
      - `ParsedNibble`
        - `ok`
          - `Console::Format::Attributes::ParsedNibble::ok` (`field`)
        - `value`
          - `Console::Format::Attributes::ParsedNibble::value` (`field`)
        - `Console::Format::Attributes::ParsedNibble` (`type`)
      - `ParsedRgb`
        - `b`
          - `Console::Format::Attributes::ParsedRgb::b` (`field`)
        - `g`
          - `Console::Format::Attributes::ParsedRgb::g` (`field`)
        - `ok`
          - `Console::Format::Attributes::ParsedRgb::ok` (`field`)
        - `r`
          - `Console::Format::Attributes::ParsedRgb::r` (`field`)
        - `Console::Format::Attributes::ParsedRgb` (`type`)
      - `ParsedU8`
        - `ok`
          - `Console::Format::Attributes::ParsedU8::ok` (`field`)
        - `value`
          - `Console::Format::Attributes::ParsedU8::value` (`field`)
        - `Console::Format::Attributes::ParsedU8` (`type`)
    - `Format`
      - `source`
        - `Console::Format::Format::source` (`parameter`)
      - `Console::Format::Format` (`function`)
    - `Markdown`
      - `IsEscapableSigil`
        - `c`
          - `Console::Format::Markdown::IsEscapableSigil::c` (`parameter`)
        - `Console::Format::Markdown::IsEscapableSigil` (`function`)
      - `RenderInner`
        - `ansi`
          - `Console::Format::Markdown::RenderInner::ansi` (`parameter`)
        - `s`
          - `Console::Format::Markdown::RenderInner::s` (`parameter`)
        - `Console::Format::Markdown::RenderInner` (`function`)
      - `RenderPlain`
        - `source`
          - `Console::Format::Markdown::RenderPlain::source` (`parameter`)
        - `Console::Format::Markdown::RenderPlain` (`function`)
      - `RenderStyled`
        - `source`
          - `Console::Format::Markdown::RenderStyled::source` (`parameter`)
        - `Console::Format::Markdown::RenderStyled` (`function`)
    - `Scan`
      - `ContainsSubstring`
        - `haystack`
          - `Console::Format::Scan::ContainsSubstring::haystack` (`parameter`)
        - `needle`
          - `Console::Format::Scan::ContainsSubstring::needle` (`parameter`)
        - `Console::Format::Scan::ContainsSubstring` (`function`)
      - `Drop`
        - `count`
          - `Console::Format::Scan::Drop::count` (`parameter`)
        - `text`
          - `Console::Format::Scan::Drop::text` (`parameter`)
        - `Console::Format::Scan::Drop` (`function`)
      - `IndexOfFrom`
        - `haystack`
          - `Console::Format::Scan::IndexOfFrom::haystack` (`parameter`)
        - `needle`
          - `Console::Format::Scan::IndexOfFrom::needle` (`parameter`)
        - `start`
          - `Console::Format::Scan::IndexOfFrom::start` (`parameter`)
        - `Console::Format::Scan::IndexOfFrom` (`function`)
      - `Len`
        - `text`
          - `Console::Format::Scan::Len::text` (`parameter`)
        - `Console::Format::Scan::Len` (`function`)
      - `Slice`
        - `count`
          - `Console::Format::Scan::Slice::count` (`parameter`)
        - `start`
          - `Console::Format::Scan::Slice::start` (`parameter`)
        - `text`
          - `Console::Format::Scan::Slice::text` (`parameter`)
        - `Console::Format::Scan::Slice` (`function`)
      - `StartsWith`
        - `prefix`
          - `Console::Format::Scan::StartsWith::prefix` (`parameter`)
        - `text`
          - `Console::Format::Scan::StartsWith::text` (`parameter`)
        - `Console::Format::Scan::StartsWith` (`function`)
      - `Trim`
        - `text`
          - `Console::Format::Scan::Trim::text` (`parameter`)
        - `Console::Format::Scan::Trim` (`function`)
      - `TrimLeft`
        - `text`
          - `Console::Format::Scan::TrimLeft::text` (`parameter`)
        - `Console::Format::Scan::TrimLeft` (`function`)
      - `TrimRight`
        - `text`
          - `Console::Format::Scan::TrimRight::text` (`parameter`)
        - `Console::Format::Scan::TrimRight` (`function`)
    - `StripMarkup`
      - `source`
        - `Console::Format::StripMarkup::source` (`parameter`)
      - `Console::Format::StripMarkup` (`function`)
    - `Console::Format` (`module`)
  - `FormatLine`
    - `text`
      - `Console::FormatLine::text` (`parameter`)
    - `Console::FormatLine` (`function`)
  - `FormatWrite`
    - `text`
      - `Console::FormatWrite::text` (`parameter`)
    - `Console::FormatWrite` (`function`)
  - `MessagesChannel`
    - `Console::MessagesChannel` (`function`)
  - `OnResize`
    - `OnResize`
      - `Console::OnResize::OnResize` (`field`)
    - `lastSize`
      - `Console::OnResize::lastSize` (`field`)
    - `Console::OnResize` (`type`)
  - `QuerySize`
    - `Console::QuerySize` (`function`)
  - `RunTick`
    - `lastSize`
      - `Console::RunTick::lastSize` (`parameter`)
    - `messages`
      - `Console::RunTick::messages` (`parameter`)
    - `Console::RunTick` (`function`)
  - `RunTickHub`
    - `hub`
      - `Console::RunTickHub::hub` (`parameter`)
    - `Console::RunTickHub` (`function`)
  - `RunTickLive`
    - `lastSize`
      - `Console::RunTickLive::lastSize` (`parameter`)
    - `live`
      - `Console::RunTickLive::live` (`parameter`)
    - `messages`
      - `Console::RunTickLive::messages` (`parameter`)
    - `Console::RunTickLive` (`function`)
  - `ShouldStyle`
    - `Console::ShouldStyle` (`function`)
  - `Start`
    - `hub`
      - `Console::Start::hub` (`parameter`)
    - `Console::Start` (`function`)
  - `Style`
    - `ClampWidth`
      - `maxValue`
        - `Console::Style::ClampWidth::maxValue` (`parameter`)
      - `minValue`
        - `Console::Style::ClampWidth::minValue` (`parameter`)
      - `value`
        - `Console::Style::ClampWidth::value` (`parameter`)
      - `Console::Style::ClampWidth` (`function`)
    - `Margin`
      - `None`
        - `Console::Style::Margin::None` (`enum_variant`)
      - `Sized`
        - `Console::Style::Margin::Sized` (`enum_variant`)
      - `Value`
        - `Console::Style::Margin::Value` (`enum_variant`)
      - `size`
        - `Console::Style::Margin::size` (`field`)
      - `value`
        - `Console::Style::Margin::value` (`field`)
      - `Console::Style::Margin` (`enum`)
    - `Padding`
      - `None`
        - `Console::Style::Padding::None` (`enum_variant`)
      - `Sized`
        - `Console::Style::Padding::Sized` (`enum_variant`)
      - `Value`
        - `Console::Style::Padding::Value` (`enum_variant`)
      - `size`
        - `Console::Style::Padding::size` (`field`)
      - `value`
        - `Console::Style::Padding::value` (`field`)
      - `Console::Style::Padding` (`enum`)
    - `ResolveMargin`
      - `columns`
        - `Console::Style::ResolveMargin::columns` (`parameter`)
      - `margin`
        - `Console::Style::ResolveMargin::margin` (`parameter`)
      - `Console::Style::ResolveMargin` (`function`)
    - `ResolvePadding`
      - `columns`
        - `Console::Style::ResolvePadding::columns` (`parameter`)
      - `padding`
        - `Console::Style::ResolvePadding::padding` (`parameter`)
      - `Console::Style::ResolvePadding` (`function`)
    - `ResolveSize`
      - `columns`
        - `Console::Style::ResolveSize::columns` (`parameter`)
      - `size`
        - `Console::Style::ResolveSize::size` (`parameter`)
      - `Console::Style::ResolveSize` (`function`)
    - `Size`
      - `Large`
        - `Console::Style::Size::Large` (`enum_variant`)
      - `Medium`
        - `Console::Style::Size::Medium` (`enum_variant`)
      - `Small`
        - `Console::Style::Size::Small` (`enum_variant`)
      - `Console::Style::Size` (`enum`)
    - `TitleAlignment`
      - `Center`
        - `Console::Style::TitleAlignment::Center` (`enum_variant`)
      - `Left`
        - `Console::Style::TitleAlignment::Left` (`enum_variant`)
      - `Right`
        - `Console::Style::TitleAlignment::Right` (`enum_variant`)
      - `Console::Style::TitleAlignment` (`enum`)
    - `Console::Style` (`module`)
  - `SubscribeOnResize`
    - `handler`
      - `Console::SubscribeOnResize::handler` (`parameter`)
    - `hub`
      - `Console::SubscribeOnResize::hub` (`parameter`)
    - `Console::SubscribeOnResize` (`function`)
  - `Console` (`module`)
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
    - `Core::String` (`module`)
- `Platform`
  - `Linux`
    - `IsAtty`
      - `fd`
        - `Platform::Linux::IsAtty::fd` (`parameter`)
      - `Platform::Linux::IsAtty` (`function`)
    - `Libc`
      - `arg`
        - `Platform::Linux::Libc::arg` (`parameter`)
      - `fd`
        - `Platform::Linux::Libc::fd` (`parameter`)
        - `Platform::Linux::Libc::fd` (`parameter`)
      - `ioctl`
        - `Platform::Linux::Libc::ioctl` (`contract_method`)
      - `isatty`
        - `Platform::Linux::Libc::isatty` (`contract_method`)
      - `request`
        - `Platform::Linux::Libc::request` (`parameter`)
      - `Platform::Linux::Libc` (`contract`)
    - `WinSize`
      - `ws_col`
        - `Platform::Linux::WinSize::ws_col` (`field`)
      - `ws_row`
        - `Platform::Linux::WinSize::ws_row` (`field`)
      - `ws_xpixel`
        - `Platform::Linux::WinSize::ws_xpixel` (`field`)
      - `ws_ypixel`
        - `Platform::Linux::WinSize::ws_ypixel` (`field`)
      - `Platform::Linux::WinSize` (`type`)
    - `Winsize`
      - `Platform::Linux::Winsize` (`function`)
  - `MacOS`
    - `IsAtty`
      - `fd`
        - `Platform::MacOS::IsAtty::fd` (`parameter`)
      - `Platform::MacOS::IsAtty` (`function`)
    - `Libc`
      - `arg`
        - `Platform::MacOS::Libc::arg` (`parameter`)
      - `fd`
        - `Platform::MacOS::Libc::fd` (`parameter`)
        - `Platform::MacOS::Libc::fd` (`parameter`)
      - `ioctl`
        - `Platform::MacOS::Libc::ioctl` (`contract_method`)
      - `isatty`
        - `Platform::MacOS::Libc::isatty` (`contract_method`)
      - `request`
        - `Platform::MacOS::Libc::request` (`parameter`)
      - `Platform::MacOS::Libc` (`contract`)
    - `WinSize`
      - `ws_col`
        - `Platform::MacOS::WinSize::ws_col` (`field`)
      - `ws_row`
        - `Platform::MacOS::WinSize::ws_row` (`field`)
      - `ws_xpixel`
        - `Platform::MacOS::WinSize::ws_xpixel` (`field`)
      - `ws_ypixel`
        - `Platform::MacOS::WinSize::ws_ypixel` (`field`)
      - `Platform::MacOS::WinSize` (`type`)
    - `Winsize`
      - `Platform::MacOS::Winsize` (`function`)
  - `Terminal`
    - `EnsureInitialized`
      - `Platform::Terminal::EnsureInitialized` (`function`)
    - `EnvEquals`
      - `expected`
        - `Platform::Terminal::EnvEquals::expected` (`parameter`)
      - `name`
        - `Platform::Terminal::EnvEquals::name` (`parameter`)
      - `Platform::Terminal::EnvEquals` (`function`)
    - `EnvFallbackSize`
      - `Platform::Terminal::EnvFallbackSize` (`function`)
    - `EnvFlagSet`
      - `name`
        - `Platform::Terminal::EnvFlagSet::name` (`parameter`)
      - `Platform::Terminal::EnvFlagSet` (`function`)
    - `ForcePlainText`
      - `Platform::Terminal::ForcePlainText` (`function`)
    - `IsAtty`
      - `fd`
        - `Platform::Terminal::IsAtty::fd` (`parameter`)
      - `Platform::Terminal::IsAtty` (`function`)
    - `ParseEnvColumns`
      - `defaultValue`
        - `Platform::Terminal::ParseEnvColumns::defaultValue` (`parameter`)
      - `value`
        - `Platform::Terminal::ParseEnvColumns::value` (`parameter`)
      - `Platform::Terminal::ParseEnvColumns` (`function`)
    - `ParseEnvRows`
      - `defaultValue`
        - `Platform::Terminal::ParseEnvRows::defaultValue` (`parameter`)
      - `value`
        - `Platform::Terminal::ParseEnvRows::value` (`parameter`)
      - `Platform::Terminal::ParseEnvRows` (`function`)
    - `PollResize`
      - `lastSize`
        - `Platform::Terminal::PollResize::lastSize` (`parameter`)
      - `messages`
        - `Platform::Terminal::PollResize::messages` (`parameter`)
      - `Platform::Terminal::PollResize` (`function`)
    - `PollResizeHub`
      - `hub`
        - `Platform::Terminal::PollResizeHub::hub` (`parameter`)
      - `Platform::Terminal::PollResizeHub` (`function`)
    - `ProbeColorModel`
      - `Platform::Terminal::ProbeColorModel` (`function`)
    - `QuerySize`
      - `Platform::Terminal::QuerySize` (`function`)
    - `Platform::Terminal` (`module`)
  - `Windows`
    - `IsAtty`
      - `fd`
        - `Platform::Windows::IsAtty::fd` (`parameter`)
      - `Platform::Windows::IsAtty` (`function`)
    - `Kernel32`
      - `GetConsoleScreenBufferInfo`
        - `Platform::Windows::Kernel32::GetConsoleScreenBufferInfo` (`contract_method`)
      - `GetStdHandle`
        - `Platform::Windows::Kernel32::GetStdHandle` (`contract_method`)
      - `handle`
        - `Platform::Windows::Kernel32::handle` (`parameter`)
      - `infoOut`
        - `Platform::Windows::Kernel32::infoOut` (`parameter`)
      - `nStdHandle`
        - `Platform::Windows::Kernel32::nStdHandle` (`parameter`)
      - `Platform::Windows::Kernel32` (`contract`)
    - `Ucrt`
      - `_isatty`
        - `Platform::Windows::Ucrt::_isatty` (`contract_method`)
      - `fd`
        - `Platform::Windows::Ucrt::fd` (`parameter`)
      - `Platform::Windows::Ucrt` (`contract`)
    - `Winsize`
      - `Platform::Windows::Winsize` (`function`)
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
    - `Query::Contracts` (`module`)
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
  - `Threading`
    - `Thread`
      - `DarwinPthread`
        - `arg`
          - `System::Threading::Thread::DarwinPthread::arg` (`parameter`)
        - `attr`
          - `System::Threading::Thread::DarwinPthread::attr` (`parameter`)
        - `pthread_create`
          - `System::Threading::Thread::DarwinPthread::pthread_create` (`contract_method`)
        - `pthread_join`
          - `System::Threading::Thread::DarwinPthread::pthread_join` (`contract_method`)
        - `sched_yield`
          - `System::Threading::Thread::DarwinPthread::sched_yield` (`contract_method`)
        - `startRoutine`
          - `System::Threading::Thread::DarwinPthread::startRoutine` (`parameter`)
        - `thread`
          - `System::Threading::Thread::DarwinPthread::thread` (`parameter`)
        - `threadOut`
          - `System::Threading::Thread::DarwinPthread::threadOut` (`parameter`)
        - `valueOut`
          - `System::Threading::Thread::DarwinPthread::valueOut` (`parameter`)
        - `System::Threading::Thread::DarwinPthread` (`contract`)
      - `Join`
        - `self`
          - `System::Threading::Thread::Join::self` (`parameter`)
        - `System::Threading::Thread::Join` (`function`)
      - `LinuxPthread`
        - `arg`
          - `System::Threading::Thread::LinuxPthread::arg` (`parameter`)
        - `attr`
          - `System::Threading::Thread::LinuxPthread::attr` (`parameter`)
        - `pthread_create`
          - `System::Threading::Thread::LinuxPthread::pthread_create` (`contract_method`)
        - `pthread_join`
          - `System::Threading::Thread::LinuxPthread::pthread_join` (`contract_method`)
        - `sched_yield`
          - `System::Threading::Thread::LinuxPthread::sched_yield` (`contract_method`)
        - `startRoutine`
          - `System::Threading::Thread::LinuxPthread::startRoutine` (`parameter`)
        - `thread`
          - `System::Threading::Thread::LinuxPthread::thread` (`parameter`)
        - `threadOut`
          - `System::Threading::Thread::LinuxPthread::threadOut` (`parameter`)
        - `valueOut`
          - `System::Threading::Thread::LinuxPthread::valueOut` (`parameter`)
        - `System::Threading::Thread::LinuxPthread` (`contract`)
      - `Spawn`
        - `arg`
          - `System::Threading::Thread::Spawn::arg` (`parameter`)
        - `entryRoutine`
          - `System::Threading::Thread::Spawn::entryRoutine` (`parameter`)
        - `System::Threading::Thread::Spawn` (`function`)
      - `Thread`
        - `handle`
          - `System::Threading::Thread::Thread::handle` (`field`)
        - `System::Threading::Thread::Thread` (`type`)
      - `Yield`
        - `System::Threading::Thread::Yield` (`function`)
      - `System::Threading::Thread` (`module`)
    - `ThreadError`
      - `ThreadError`
        - `SpawnFailed`
          - `System::Threading::ThreadError::ThreadError::SpawnFailed` (`enum_variant`)
        - `code`
          - `System::Threading::ThreadError::ThreadError::code` (`field`)
        - `System::Threading::ThreadError::ThreadError` (`enum`)
      - `System::Threading::ThreadError` (`module`)
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
- `range`
  - `range` (`function`)

## Items

### `Ansi::Contracts::AnsiCursorStep` (`contract`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiCursorStep::Column` (`contract_method`)

Moves cursor to column # (`CSI # G`).

---

### `Ansi::Contracts::AnsiCursorStep::Down` (`contract_method`)

Moves cursor down # lines (`CSI # B`).

---

### `Ansi::Contracts::AnsiCursorStep::Home` (`contract_method`)

Moves cursor to home (0, 0) (`CSI H`).

---

### `Ansi::Contracts::AnsiCursorStep::IntoSequence` (`contract_method`)

Concatenated escape bytes for the chain.

---

### `Ansi::Contracts::AnsiCursorStep::Left` (`contract_method`)

Moves cursor left # columns (`CSI # D`).

---

### `Ansi::Contracts::AnsiCursorStep::NextLine` (`contract_method`)

Moves cursor to beginning of next line, # lines down (`CSI # E`).

---

### `Ansi::Contracts::AnsiCursorStep::Position` (`contract_method`)

Moves cursor to row/column (`CSI row;col H`).

---

### `Ansi::Contracts::AnsiCursorStep::PrevLine` (`contract_method`)

Moves cursor to beginning of previous line, # lines up (`CSI # F`).

---

### `Ansi::Contracts::AnsiCursorStep::RestoreDec` (`contract_method`)

DEC restore cursor (`ESC 8`).

---

### `Ansi::Contracts::AnsiCursorStep::Right` (`contract_method`)

Moves cursor right # columns (`CSI # C`).

---

### `Ansi::Contracts::AnsiCursorStep::SaveDec` (`contract_method`)

DEC save cursor (`ESC 7`).

---

### `Ansi::Contracts::AnsiCursorStep::Up` (`contract_method`)

Moves cursor up # lines (`CSI # A`).

---

### `Ansi::Contracts::AnsiCursorStep::col` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiCursorStep::col` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiCursorStep::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiCursorStep::row` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiEraseStep` (`contract`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiEraseStep::DisplayAll` (`contract_method`)

Erase entire display (`CSI 2 J`).

---

### `Ansi::Contracts::AnsiEraseStep::DisplayFromCursor` (`contract_method`)

Erase from cursor to end of display (`CSI 0 J`).

---

### `Ansi::Contracts::AnsiEraseStep::DisplaySaved` (`contract_method`)

Erase saved lines (`CSI 3 J`).

---

### `Ansi::Contracts::AnsiEraseStep::DisplayToCursor` (`contract_method`)

Erase from cursor to beginning of display (`CSI 1 J`).

---

### `Ansi::Contracts::AnsiEraseStep::IntoSequence` (`contract_method`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiEraseStep::LineAll` (`contract_method`)

Erase entire line (`CSI 2 K`).

---

### `Ansi::Contracts::AnsiEraseStep::LineFromCursor` (`contract_method`)

Erase from cursor to end of line (`CSI 0 K`).

---

### `Ansi::Contracts::AnsiEraseStep::LineToCursor` (`contract_method`)

Erase from start of line to cursor (`CSI 1 K`).

---

### `Ansi::Contracts::AnsiInputModeStep` (`contract`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiInputModeStep::DisableMouseClick` (`contract_method`)

Disables basic mouse click reporting (`CSI ?1000 l`).

---

### `Ansi::Contracts::AnsiInputModeStep::DisableMouseDrag` (`contract_method`)

Disables button-event mouse tracking (`CSI ?1002 l`).

---

### `Ansi::Contracts::AnsiInputModeStep::DisableMouseMotion` (`contract_method`)

Disables any-motion mouse tracking (`CSI ?1003 l`).

---

### `Ansi::Contracts::AnsiInputModeStep::DisableSgrMouse` (`contract_method`)

Disables SGR extended mouse coordinates (`CSI ?1006 l`).

---

### `Ansi::Contracts::AnsiInputModeStep::EnableMouseClick` (`contract_method`)

Enables basic mouse click reporting (`CSI ?1000 h`).

---

### `Ansi::Contracts::AnsiInputModeStep::EnableMouseDrag` (`contract_method`)

Enables button-event mouse tracking (`CSI ?1002 h`).

---

### `Ansi::Contracts::AnsiInputModeStep::EnableMouseMotion` (`contract_method`)

Enables any-motion mouse tracking (`CSI ?1003 h`).

---

### `Ansi::Contracts::AnsiInputModeStep::EnableSgrMouse` (`contract_method`)

Enables SGR extended mouse coordinates (`CSI ?1006 h`).

---

### `Ansi::Contracts::AnsiInputModeStep::IntoSequence` (`contract_method`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiInputModeStep::RedefineKey` (`contract_method`)

Redefines a keyboard key (`CSI code ; string p`).

---

### `Ansi::Contracts::AnsiInputModeStep::binding` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiInputModeStep::code` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiOscStep` (`contract`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiOscStep::Hyperlink` (`contract_method`)

Wraps label with OSC 8 hyperlink open/close.

---

### `Ansi::Contracts::AnsiOscStep::IntoSequence` (`contract_method`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiOscStep::SetTitle` (`contract_method`)

Sets window/icon title (`OSC 0 ; title BEL`).

---

### `Ansi::Contracts::AnsiOscStep::label` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiOscStep::title` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiOscStep::uri` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiScreenStep` (`contract`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiScreenStep::DisableAltScreen` (`contract_method`)

Disables alternative screen buffer (`CSI ?1049 l`).

---

### `Ansi::Contracts::AnsiScreenStep::DisableLineWrap` (`contract_method`)

Disables line wrapping (`CSI ?7 l`).

---

### `Ansi::Contracts::AnsiScreenStep::EnableAltScreen` (`contract_method`)

Enables alternative screen buffer (`CSI ?1049 h`).

---

### `Ansi::Contracts::AnsiScreenStep::EnableLineWrap` (`contract_method`)

Enables line wrapping (`CSI ?7 h`).

---

### `Ansi::Contracts::AnsiScreenStep::HideCursor` (`contract_method`)

Hides cursor (`CSI ?25 l`).

---

### `Ansi::Contracts::AnsiScreenStep::IntoSequence` (`contract_method`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiScreenStep::RestoreScreen` (`contract_method`)

Restores screen (`CSI ?47 l`).

---

### `Ansi::Contracts::AnsiScreenStep::SaveScreen` (`contract_method`)

Saves screen (`CSI ?47 h`).

---

### `Ansi::Contracts::AnsiScreenStep::ScrollRegion` (`contract_method`)

Sets scroll region top/bottom inclusive (`CSI top;bottom r`).

---

### `Ansi::Contracts::AnsiScreenStep::ShowCursor` (`contract_method`)

Shows cursor (`CSI ?25 h`).

---

### `Ansi::Contracts::AnsiScreenStep::bottom` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiScreenStep::top` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiStyleStep` (`contract`)

Fluent ANSI builder contracts (implementations return the same step type).

---

### `Ansi::Contracts::AnsiStyleStep::ApplyTo` (`contract_method`)

Wraps plain text with open SGR and trailing reset (`0m`).

---

### `Ansi::Contracts::AnsiStyleStep::Bg256` (`contract_method`)

Sets 256-color background (`48;5;n`).

---

### `Ansi::Contracts::AnsiStyleStep::BgBasic` (`contract_method`)

Sets 8/16-color background (40–47, 100–107).

---

### `Ansi::Contracts::AnsiStyleStep::BgRgb` (`contract_method`)

Sets 24-bit background (`48;2;r;g;b`), downgraded per effective color model.

---

### `Ansi::Contracts::AnsiStyleStep::Bold` (`contract_method`)

Applies bold SGR (1).

---

### `Ansi::Contracts::AnsiStyleStep::Dim` (`contract_method`)

Applies dim SGR (2).

---

### `Ansi::Contracts::AnsiStyleStep::Fg256` (`contract_method`)

Sets 256-color foreground (`38;5;n`).

---

### `Ansi::Contracts::AnsiStyleStep::FgBasic` (`contract_method`)

Sets 8/16-color foreground (30–37, 90–97).

---

### `Ansi::Contracts::AnsiStyleStep::FgRgb` (`contract_method`)

Sets 24-bit foreground (`38;2;r;g;b`), downgraded per effective color model.

---

### `Ansi::Contracts::AnsiStyleStep::IntoPrefix` (`contract_method`)

Returns only the escape prefix (no text, no reset).

---

### `Ansi::Contracts::AnsiStyleStep::Inverse` (`contract_method`)

Applies inverse SGR (7).

---

### `Ansi::Contracts::AnsiStyleStep::Italic` (`contract_method`)

Applies italic SGR (3).

---

### `Ansi::Contracts::AnsiStyleStep::Strike` (`contract_method`)

Applies strikethrough SGR (9).

---

### `Ansi::Contracts::AnsiStyleStep::Underline` (`contract_method`)

Applies underline SGR (4).

---

### `Ansi::Contracts::AnsiStyleStep::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiStyleStep::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiStyleStep::code` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiStyleStep::code` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiStyleStep::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiStyleStep::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiStyleStep::index` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiStyleStep::index` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiStyleStep::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiStyleStep::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::Contracts::AnsiStyleStep::text` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor` (`module`)

*No documentation provided.*

---

### `Ansi::Cursor::Append` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::Append::fragment` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Append::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Column` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::Column::col` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Column::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::CursorBuilder` (`type`)

*No documentation provided.*

---

### `Ansi::Cursor::CursorBuilder::parts` (`field`)

*No documentation provided.*

---

### `Ansi::Cursor::Down` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::Down::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Down::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Home` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::Home::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::IntoSequence` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::IntoSequence::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Left` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::Left::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Left::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::NextLine` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::NextLine::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::NextLine::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Position` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::Position::col` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Position::row` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Position::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::PrevLine` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::PrevLine::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::PrevLine::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::RestoreDec` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::RestoreDec::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Right` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::Right::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Right::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::SaveDec` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::SaveDec::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Start` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::Up` (`function`)

*No documentation provided.*

---

### `Ansi::Cursor::Up::count` (`parameter`)

*No documentation provided.*

---

### `Ansi::Cursor::Up::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Erase` (`module`)

*No documentation provided.*

---

### `Ansi::Erase::Append` (`function`)

*No documentation provided.*

---

### `Ansi::Erase::Append::fragment` (`parameter`)

*No documentation provided.*

---

### `Ansi::Erase::Append::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Erase::DisplayAll` (`function`)

*No documentation provided.*

---

### `Ansi::Erase::DisplayAll::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Erase::DisplayFromCursor` (`function`)

*No documentation provided.*

---

### `Ansi::Erase::DisplayFromCursor::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Erase::DisplaySaved` (`function`)

*No documentation provided.*

---

### `Ansi::Erase::DisplaySaved::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Erase::DisplayToCursor` (`function`)

*No documentation provided.*

---

### `Ansi::Erase::DisplayToCursor::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Erase::EraseBuilder` (`type`)

*No documentation provided.*

---

### `Ansi::Erase::EraseBuilder::parts` (`field`)

*No documentation provided.*

---

### `Ansi::Erase::IntoSequence` (`function`)

*No documentation provided.*

---

### `Ansi::Erase::IntoSequence::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Erase::LineAll` (`function`)

*No documentation provided.*

---

### `Ansi::Erase::LineAll::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Erase::LineFromCursor` (`function`)

*No documentation provided.*

---

### `Ansi::Erase::LineFromCursor::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Erase::LineToCursor` (`function`)

*No documentation provided.*

---

### `Ansi::Erase::LineToCursor::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Erase::Start` (`function`)

*No documentation provided.*

---

### `Ansi::Escape` (`module`)

*No documentation provided.*

---

### `Ansi::Escape::Csi` (`function`)

Builds a CSI sequence without capability gating (for composition and golden tests).

---

### `Ansi::Escape::Csi::body` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::Csi::finalByte` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::CsiOpen` (`function`)

*No documentation provided.*

---

### `Ansi::Escape::CsiSequence` (`function`)

*No documentation provided.*

---

### `Ansi::Escape::CsiSequence::body` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::CsiSequence::finalByte` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::DecRestoreCursor` (`function`)

*No documentation provided.*

---

### `Ansi::Escape::DecSaveCursor` (`function`)

DEC save/restore cursor (`ESC 7` / `ESC 8`).

---

### `Ansi::Escape::EmitCsi` (`function`)

Capability-gated CSI sequence.

---

### `Ansi::Escape::EmitCsi::body` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::EmitCsi::finalByte` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::EmitDec` (`function`)

Capability-gated DEC escape.

---

### `Ansi::Escape::EmitDec::suffix` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::EmitOsc` (`function`)

Capability-gated OSC sequence.

---

### `Ansi::Escape::EmitOsc::payload` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::Esc` (`function`)

Returns the ASCII ESC control character (U+001B).

---

### `Ansi::Escape::JoinArgs` (`function`)

*No documentation provided.*

---

### `Ansi::Escape::JoinArgs::a` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::JoinArgs::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::OscSequence` (`function`)

OSC payload terminated with BEL (0x07).

---

### `Ansi::Escape::OscSequence::payload` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::PrivateMode` (`function`)

CSI private-mode set (`h`) or reset (`l`), e.g. `?1049h`.

---

### `Ansi::Escape::PrivateMode::enable` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::PrivateMode::mode` (`parameter`)

*No documentation provided.*

---

### `Ansi::Escape::WhenEnabled` (`function`)

Returns `sequence` when ANSI emission is allowed, otherwise `""`.

---

### `Ansi::Escape::WhenEnabled::sequence` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode` (`module`)

*No documentation provided.*

---

### `Ansi::InputMode::Append` (`function`)

*No documentation provided.*

---

### `Ansi::InputMode::Append::fragment` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::Append::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::DisableMouseClick` (`function`)

*No documentation provided.*

---

### `Ansi::InputMode::DisableMouseClick::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::DisableMouseDrag` (`function`)

*No documentation provided.*

---

### `Ansi::InputMode::DisableMouseDrag::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::DisableMouseMotion` (`function`)

*No documentation provided.*

---

### `Ansi::InputMode::DisableMouseMotion::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::DisableSgrMouse` (`function`)

*No documentation provided.*

---

### `Ansi::InputMode::DisableSgrMouse::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::EnableMouseClick` (`function`)

*No documentation provided.*

---

### `Ansi::InputMode::EnableMouseClick::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::EnableMouseDrag` (`function`)

*No documentation provided.*

---

### `Ansi::InputMode::EnableMouseDrag::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::EnableMouseMotion` (`function`)

*No documentation provided.*

---

### `Ansi::InputMode::EnableMouseMotion::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::EnableSgrMouse` (`function`)

*No documentation provided.*

---

### `Ansi::InputMode::EnableSgrMouse::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::InputModeBuilder` (`type`)

*No documentation provided.*

---

### `Ansi::InputMode::InputModeBuilder::parts` (`field`)

*No documentation provided.*

---

### `Ansi::InputMode::IntoSequence` (`function`)

*No documentation provided.*

---

### `Ansi::InputMode::IntoSequence::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::RedefineKey` (`function`)

*No documentation provided.*

---

### `Ansi::InputMode::RedefineKey::binding` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::RedefineKey::code` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::RedefineKey::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::InputMode::Start` (`function`)

*No documentation provided.*

---

### `Ansi::Osc` (`module`)

*No documentation provided.*

---

### `Ansi::Osc::Append` (`function`)

*No documentation provided.*

---

### `Ansi::Osc::Append::fragment` (`parameter`)

*No documentation provided.*

---

### `Ansi::Osc::Append::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Osc::Hyperlink` (`function`)

*No documentation provided.*

---

### `Ansi::Osc::Hyperlink::label` (`parameter`)

*No documentation provided.*

---

### `Ansi::Osc::Hyperlink::uri` (`parameter`)

*No documentation provided.*

---

### `Ansi::Osc::IntoSequence` (`function`)

*No documentation provided.*

---

### `Ansi::Osc::IntoSequence::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Osc::OscBuilder` (`type`)

*No documentation provided.*

---

### `Ansi::Osc::OscBuilder::parts` (`field`)

*No documentation provided.*

---

### `Ansi::Osc::SetTitle` (`function`)

*No documentation provided.*

---

### `Ansi::Osc::SetTitle::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Osc::SetTitle::title` (`parameter`)

*No documentation provided.*

---

### `Ansi::Osc::Start` (`function`)

*No documentation provided.*

---

### `Ansi::Screen` (`module`)

*No documentation provided.*

---

### `Ansi::Screen::Append` (`function`)

*No documentation provided.*

---

### `Ansi::Screen::Append::fragment` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::Append::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::DisableAltScreen` (`function`)

*No documentation provided.*

---

### `Ansi::Screen::DisableAltScreen::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::DisableLineWrap` (`function`)

*No documentation provided.*

---

### `Ansi::Screen::DisableLineWrap::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::EnableAltScreen` (`function`)

*No documentation provided.*

---

### `Ansi::Screen::EnableAltScreen::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::EnableLineWrap` (`function`)

*No documentation provided.*

---

### `Ansi::Screen::EnableLineWrap::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::HideCursor` (`function`)

*No documentation provided.*

---

### `Ansi::Screen::HideCursor::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::IntoSequence` (`function`)

*No documentation provided.*

---

### `Ansi::Screen::IntoSequence::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::RestoreScreen` (`function`)

*No documentation provided.*

---

### `Ansi::Screen::RestoreScreen::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::SaveScreen` (`function`)

*No documentation provided.*

---

### `Ansi::Screen::SaveScreen::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::ScreenBuilder` (`type`)

*No documentation provided.*

---

### `Ansi::Screen::ScreenBuilder::parts` (`field`)

*No documentation provided.*

---

### `Ansi::Screen::ScrollRegion` (`function`)

*No documentation provided.*

---

### `Ansi::Screen::ScrollRegion::bottom` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::ScrollRegion::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::ScrollRegion::top` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::ShowCursor` (`function`)

*No documentation provided.*

---

### `Ansi::Screen::ShowCursor::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Screen::Start` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr` (`module`)

*No documentation provided.*

---

### `Ansi::Sgr::ApplyTo` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::ApplyTo::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::ApplyTo::text` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::BackgroundColorArgs` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::BackgroundColorArgs::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::BackgroundColorArgs::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::BackgroundColorArgs::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::Bg256` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::Bg256::index` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::Bg256::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::BgBasic` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::BgBasic::code` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::BgBasic::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::BgRgb` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::BgRgb::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::BgRgb::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::BgRgb::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::BgRgb::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::Bold` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::Bold::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::ClampChannelBucket` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::ClampChannelBucket::channel` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::Dim` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::Dim::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::DominantChannelIndex` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::DominantChannelIndex::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::DominantChannelIndex::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::DominantChannelIndex::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::Fg256` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::Fg256::index` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::Fg256::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::FgBasic` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::FgBasic::code` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::FgBasic::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::FgRgb` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::FgRgb::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::FgRgb::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::FgRgb::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::FgRgb::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::ForegroundColorArgs` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::ForegroundColorArgs::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::ForegroundColorArgs::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::ForegroundColorArgs::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::IntoPrefix` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::IntoPrefix::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::Inverse` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::Inverse::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::Italic` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::Italic::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbTo256Index` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbTo256Index::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbTo256Index::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbTo256Index::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbToBasicBackground` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbToBasicBackground::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbToBasicBackground::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbToBasicBackground::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbToBasicForeground` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbToBasicForeground::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbToBasicForeground::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::RgbToBasicForeground::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::SgrBuilder` (`type`)

*No documentation provided.*

---

### `Ansi::Sgr::SgrBuilder::openArgs` (`field`)

*No documentation provided.*

---

### `Ansi::Sgr::Start` (`function`)

Starts a new SGR chain.

---

### `Ansi::Sgr::Strike` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::Strike::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::Sgr::Underline` (`function`)

*No documentation provided.*

---

### `Ansi::Sgr::Underline::self` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain` (`module`)

*No documentation provided.*

---

### `Ansi::StyleChain::AppendCode` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::AppendCode::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::AppendCode::code` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Apply` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Apply::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Apply::text` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::ApplyTo` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::ApplyTo::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::ApplyTo::text` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Background256` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Background256::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Background256::index` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::BackgroundRgb` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::BackgroundRgb::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::BackgroundRgb::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::BackgroundRgb::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::BackgroundRgb::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Bg256` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Bg256::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Bg256::index` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::BgBasic` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::BgBasic::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::BgBasic::code` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::BgRgb` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::BgRgb::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::BgRgb::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::BgRgb::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::BgRgb::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Bold` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Bold::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Dim` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Dim::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Fg256` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Fg256::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Fg256::index` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::FgBasic` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::FgBasic::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::FgBasic::code` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::FgRgb` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::FgRgb::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::FgRgb::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::FgRgb::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::FgRgb::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Foreground256` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Foreground256::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Foreground256::index` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::ForegroundRgb` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::ForegroundRgb::b` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::ForegroundRgb::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::ForegroundRgb::g` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::ForegroundRgb::r` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::IntoPrefix` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::IntoPrefix::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Inverse` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Inverse::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Italic` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Italic::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::New` (`function`)

Starts a new style chain.

---

### `Ansi::StyleChain::Open` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Open::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::Reset` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Strike` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Strike::chain` (`parameter`)

*No documentation provided.*

---

### `Ansi::StyleChain::StyleChain` (`type`)

*No documentation provided.*

---

### `Ansi::StyleChain::StyleChain::openCodes` (`field`)

*No documentation provided.*

---

### `Ansi::StyleChain::Underline` (`function`)

*No documentation provided.*

---

### `Ansi::StyleChain::Underline::chain` (`parameter`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect` (`module`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AnalysisRequest` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AnalysisResult` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Analyzer` (`contract`)

Post-semantic diagnostic and rewrite-registration entrypoint.

---

### `Beskid::Compiler::Collect::Analyzer::Analyze` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Analyzer::request` (`parameter`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AttributeDeclarationSet` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AttributeGenerationRequest` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AttributeGenerator` (`contract`)

Attribute declarations exported by Mod packages.

---

### `Beskid::Compiler::Collect::AttributeGenerator::Attributes` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::AttributeGenerator::request` (`parameter`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::CollectFacadeVersion` (`function`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::CollectRequest` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::CollectTargetSet` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Collector` (`contract`)

Declarative target collection and scope narrowing for a Mod instance.

---

### `Beskid::Compiler::Collect::Collector::Collect` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Collector::request` (`parameter`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::FixError` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::GeneratedSyntaxContribution` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::GenerationRequest` (`type`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Generator` (`contract`)

Incremental typed AST contribution entrypoint.

---

### `Beskid::Compiler::Collect::Generator::Generate` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Generator::request` (`parameter`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Rewriter` (`contract`)

Typed replacement contract. TSourceNode and TTargetNode are SDK type parameters until contract generics are admitted by the grammar.

---

### `Beskid::Compiler::Collect::Rewriter::Rewrite` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Compiler::Collect::Rewriter::sourceNode` (`parameter`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation` (`module`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::CompilerLanguageVersionToken` (`function`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::ModSdkCompilationSurfaceVersion` (`function`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane` (`enum`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::Hir` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::SemanticSnapshot` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::ReflectSdkCompilationSnapshotPlane::SyntaxTree` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::Compilation::SemanticSnapshotFamilyToken` (`function`)

*No documentation provided.*

---

### `Beskid::Compiler::Diagnostics` (`module`)

*No documentation provided.*

---

### `Beskid::Compiler::Query` (`module`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter` (`module`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind` (`enum`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::DiagnosticAttachment` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::LoweringDirective` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter::ReflectSdkEmitContributionKind::Metadata` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Compiler::TypedEmitter::TypedEmitterFacadeVersion` (`function`)

*No documentation provided.*

---

### `Beskid::Syntax` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ArrayLiteralExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ArrayLiteralExpression::ArrayLiteralExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/array_literal_expression.rs` — `ArrayLiteralExpression`.

**Rust documentation** (from mirrored type):
`[elem0, elem1, ...]` — array literal expression.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `elements`.

---

### `Beskid::Syntax::Nodes::ArrayLiteralExpression::ArrayLiteralExpression::elements` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AssignExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AssignExpression::AssignExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/assign_expression.rs` — `AssignExpression`.

**Rust documentation** (from mirrored type):
Assignment or compound assignment (`=`, `+=`, `-=`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `target`, `value`.

---

### `Beskid::Syntax::Nodes::AssignExpression::AssignExpression::target` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AssignExpression::AssignExpression::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AssignOp` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AssignOp::AssignOp` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/assign_expression.rs` — `AssignOp`.

**Rust documentation** (from mirrored type):
Compound or simple assignment operator token.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Assign`**
unit (no payload)


**Variant `AddAssign`**
unit (no payload)


**Variant `SubAssign`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::AssignOp::AssignOp::AddAssign` (`enum_variant`)



**Variant `AddAssign`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::AssignOp::AssignOp::Assign` (`enum_variant`)



**Variant `Assign`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::AssignOp::AssignOp::SubAssign` (`enum_variant`)



**Variant `SubAssign`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Attribute` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Attribute::Attribute` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/attribute.rs` — `Attribute`.

**Rust documentation** (from mirrored type):
Attribute instance with optional named arguments (`Name(arg = value, ...)`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `arguments`.

---

### `Beskid::Syntax::Nodes::Attribute::Attribute::arguments` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Attribute::Attribute::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeArgument` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/attribute.rs` — `AttributeArgument`.

**Rust documentation** (from mirrored type):
Named argument supplied when applying an attribute.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `value`.

---

### `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeArgument::AttributeArgument::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeArgumentList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeDeclaration` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/attribute.rs` — `AttributeDeclaration`.

**Rust documentation** (from mirrored type):
Declaration of a reusable attribute kind (targets and parameters).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `name`, `targets`, `parameters`.

---

### `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::targets` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeDeclaration::AttributeDeclaration::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeParameter` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/attribute.rs` — `AttributeParameter`.

**Rust documentation** (from mirrored type):
Parameter slot on an attribute declaration (name, type, optional default).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `ty`, `defaultValue`.

---

### `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::defaultValue` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeParameter::AttributeParameter::ty` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeParameterList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeTarget` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeTarget::AttributeTarget` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/attribute.rs` — `AttributeTarget`.

**Rust documentation** (from mirrored type):
Syntactic placement target for an attribute (`fn`, `type`, ...).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`.

---

### `Beskid::Syntax::Nodes::AttributeTarget::AttributeTarget::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::AttributeTargetList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/binary_expression.rs` — `BinaryExpression`.

**Rust documentation** (from mirrored type):
Binary operator expression with left and right operands.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `left`, `op`, `right`.

---

### `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::left` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::op` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryExpression::BinaryExpression::right` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryOp` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/binary_expression.rs` — `BinaryOp`.

**Rust documentation** (from mirrored type):
Supported binary operators (logical, comparison, arithmetic).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Or`**
unit (no payload)


**Variant `And`**
unit (no payload)


**Variant `IdentityEq`**
unit (no payload)


**Variant `IdentityNotEq`**
unit (no payload)


**Variant `Eq`**
unit (no payload)


**Variant `NotEq`**
unit (no payload)


**Variant `Lt`**
unit (no payload)


**Variant `Lte`**
unit (no payload)


**Variant `Gt`**
unit (no payload)


**Variant `Gte`**
unit (no payload)


**Variant `Add`**
unit (no payload)


**Variant `Sub`**
unit (no payload)


**Variant `Mul`**
unit (no payload)


**Variant `Div`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Add` (`enum_variant`)



**Variant `Add`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::And` (`enum_variant`)



**Variant `And`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Div` (`enum_variant`)



**Variant `Div`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Eq` (`enum_variant`)



**Variant `Eq`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Gt` (`enum_variant`)



**Variant `Gt`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Gte` (`enum_variant`)



**Variant `Gte`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::IdentityEq` (`enum_variant`)



**Variant `IdentityEq`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::IdentityNotEq` (`enum_variant`)



**Variant `IdentityNotEq`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Lt` (`enum_variant`)



**Variant `Lt`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Lte` (`enum_variant`)



**Variant `Lte`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Mul` (`enum_variant`)



**Variant `Mul`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::NotEq` (`enum_variant`)



**Variant `NotEq`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Or` (`enum_variant`)



**Variant `Or`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::BinaryOp::BinaryOp::Sub` (`enum_variant`)



**Variant `Sub`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Block` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Block::Block` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/block.rs` — `Block`.

**Rust documentation** (from mirrored type):
Braced sequence of statements.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `statements`.

---

### `Beskid::Syntax::Nodes::Block::Block::statements` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BlockExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BlockExpression::BlockExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/block_expression.rs` — `BlockExpression`.

**Rust documentation** (from mirrored type):
Block used as an expression (`{ ... }` value).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `block`.

---

### `Beskid::Syntax::Nodes::BlockExpression::BlockExpression::block` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BreakStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::BreakStatement::BreakStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/break_statement.rs` — `BreakStatement`.

**Rust documentation** (from mirrored type):
`break` out of the nearest enclosing loop.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Marker struct with no fields.

---

### `Beskid::Syntax::Nodes::CallExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::CallExpression::CallExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/call_expression.rs` — `CallExpression`.

**Rust documentation** (from mirrored type):
Function- or method-style call with positional arguments.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `callee`, `args`.

---

### `Beskid::Syntax::Nodes::CallExpression::CallExpression::args` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::CallExpression::CallExpression::callee` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContinueStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContinueStatement::ContinueStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/continue_statement.rs` — `ContinueStatement`.

**Rust documentation** (from mirrored type):
`continue` to the next iteration of the nearest enclosing loop.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Marker struct with no fields.

---

### `Beskid::Syntax::Nodes::ContractDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/contract_definition.rs` — `ContractDefinition`.

**Rust documentation** (from mirrored type):
`contract` interface: members (method signatures and embeddings) with per-item docs.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `attributes`, `visibility`, `name`, `items`.

---

### `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::attributes` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::items` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractDefinition::ContractDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractEmbedding` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractEmbedding::ContractEmbedding` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/contract_embedding.rs` — `ContractEmbedding`.

**Rust documentation** (from mirrored type):
Contract member that embeds another contract by name.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`.

---

### `Beskid::Syntax::Nodes::ContractEmbedding::ContractEmbedding::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractMethodSignature` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/contract_method_signature.rs` — `ContractMethodSignature`.

**Rust documentation** (from mirrored type):
Abstract method signature inside a `contract` (no body).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `parameters`, `returnType`.

---

### `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractMethodSignature::ContractMethodSignature::returnType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractNode` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractNode::ContractNode` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/contract_node.rs` — `ContractNode`.

**Rust documentation** (from mirrored type):
Member of a contract: method signature or embedding.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `MethodSignature`**
tuple (payload: Beskid.Syntax.Nodes.ContractMethodSignature)


**Variant `Embedding`**
tuple (payload: Beskid.Syntax.Nodes.ContractEmbedding)


---

### `Beskid::Syntax::Nodes::ContractNode::ContractNode::Embedding` (`enum_variant`)



**Variant `Embedding`**
tuple payload: payload (Beskid.Syntax.Nodes.ContractEmbedding).


---

### `Beskid::Syntax::Nodes::ContractNode::ContractNode::MethodSignature` (`enum_variant`)



**Variant `MethodSignature`**
tuple payload: payload (Beskid.Syntax.Nodes.ContractMethodSignature).


---

### `Beskid::Syntax::Nodes::ContractNode::ContractNode::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractNode::ContractNode::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ContractNodeList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Descendants` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Descendants::Descendants` (`contract`)

Pre-order descendant iterator contract (lowers to `beskid_analysis::query::Descendants`).

---

### `Beskid::Syntax::Nodes::Descendants::Descendants::Current` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Descendants::Descendants::MoveNext` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumConstructorExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/enum_constructor_expression.rs` — `EnumConstructorExpression`.

**Rust documentation** (from mirrored type):
Enum variant construction `Type.Variant(args...)`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `path`, `args`.

---

### `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression::args` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumConstructorExpression::EnumConstructorExpression::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/enum_definition.rs` — `EnumDefinition`.

**Rust documentation** (from mirrored type):
`enum` definition with variants and optional generic parameters.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `name`, `generics`, `variants`.

---

### `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::generics` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::variants` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumDefinition::EnumDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPath` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPath::EnumPath` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/enum_path.rs` — `EnumPath`.

**Rust documentation** (from mirrored type):
Qualified path naming an enum variant (`Module.Type::Variant`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `typePath`, `variant`.

---

### `Beskid::Syntax::Nodes::EnumPath::EnumPath::typePath` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPath::EnumPath::variant` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPattern` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPattern::EnumPattern` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/pattern.rs` — `EnumPattern`.

**Rust documentation** (from mirrored type):
Enum variant pattern with optional nested sub-patterns.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `path`, `items`.

---

### `Beskid::Syntax::Nodes::EnumPattern::EnumPattern::items` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumPattern::EnumPattern::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumVariant` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumVariant::EnumVariant` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/enum_variant.rs` — `EnumVariant`.

**Rust documentation** (from mirrored type):
Single enum variant and its field list.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `fields`.

---

### `Beskid::Syntax::Nodes::EnumVariant::EnumVariant::fields` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumVariant::EnumVariant::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::EnumVariantList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/expression.rs` — `Expression`.

**Rust documentation** (from mirrored type):
Top-level expression shape after parsing (postfix chains, operators, literals, etc.).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Match`**
tuple (payload: Beskid.Syntax.Nodes.MatchExpression)


**Variant `Lambda`**
tuple (payload: Beskid.Syntax.Nodes.LambdaExpression)


**Variant `Assign`**
tuple (payload: Beskid.Syntax.Nodes.AssignExpression)


**Variant `Binary`**
tuple (payload: Beskid.Syntax.Nodes.BinaryExpression)


**Variant `Unary`**
tuple (payload: Beskid.Syntax.Nodes.UnaryExpression)


**Variant `Call`**
tuple (payload: Beskid.Syntax.Nodes.CallExpression)


**Variant `Member`**
tuple (payload: Beskid.Syntax.Nodes.MemberExpression)


**Variant `Literal`**
tuple (payload: Beskid.Syntax.Nodes.LiteralExpression)


**Variant `Path`**
tuple (payload: Beskid.Syntax.Nodes.PathExpression)


**Variant `StructLiteral`**
tuple (payload: Beskid.Syntax.Nodes.StructLiteralExpression)


**Variant `EnumConstructor`**
tuple (payload: Beskid.Syntax.Nodes.EnumConstructorExpression)


**Variant `Block`**
tuple (payload: Beskid.Syntax.Nodes.BlockExpression)


**Variant `Grouped`**
tuple (payload: Beskid.Syntax.Nodes.GroupedExpression)


**Variant `Try`**
tuple (payload: Beskid.Syntax.Nodes.TryExpression)


**Variant `Spawn`**
tuple (payload: Beskid.Syntax.Nodes.SpawnExpression)


**Variant `MacroInvocation`**
tuple (payload: Beskid.Syntax.Nodes.MacroInvocation)


**Variant `MacroMetavariable`**
tuple (payload: Beskid.Syntax.Nodes.MacroMetavariable)


**Variant `Index`**
tuple (payload: Beskid.Syntax.Nodes.IndexExpression)


**Variant `ArrayLiteral`**
tuple (payload: Beskid.Syntax.Nodes.ArrayLiteralExpression)


---

### `Beskid::Syntax::Nodes::Expression::Expression::ArrayLiteral` (`enum_variant`)



**Variant `ArrayLiteral`**
tuple payload: payload (Beskid.Syntax.Nodes.ArrayLiteralExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Assign` (`enum_variant`)



**Variant `Assign`**
tuple payload: payload (Beskid.Syntax.Nodes.AssignExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Binary` (`enum_variant`)



**Variant `Binary`**
tuple payload: payload (Beskid.Syntax.Nodes.BinaryExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Block` (`enum_variant`)



**Variant `Block`**
tuple payload: payload (Beskid.Syntax.Nodes.BlockExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Call` (`enum_variant`)



**Variant `Call`**
tuple payload: payload (Beskid.Syntax.Nodes.CallExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::EnumConstructor` (`enum_variant`)



**Variant `EnumConstructor`**
tuple payload: payload (Beskid.Syntax.Nodes.EnumConstructorExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Grouped` (`enum_variant`)



**Variant `Grouped`**
tuple payload: payload (Beskid.Syntax.Nodes.GroupedExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Index` (`enum_variant`)



**Variant `Index`**
tuple payload: payload (Beskid.Syntax.Nodes.IndexExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Lambda` (`enum_variant`)



**Variant `Lambda`**
tuple payload: payload (Beskid.Syntax.Nodes.LambdaExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Literal` (`enum_variant`)



**Variant `Literal`**
tuple payload: payload (Beskid.Syntax.Nodes.LiteralExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::MacroInvocation` (`enum_variant`)



**Variant `MacroInvocation`**
tuple payload: payload (Beskid.Syntax.Nodes.MacroInvocation).


---

### `Beskid::Syntax::Nodes::Expression::Expression::MacroMetavariable` (`enum_variant`)



**Variant `MacroMetavariable`**
tuple payload: payload (Beskid.Syntax.Nodes.MacroMetavariable).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Match` (`enum_variant`)



**Variant `Match`**
tuple payload: payload (Beskid.Syntax.Nodes.MatchExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Member` (`enum_variant`)



**Variant `Member`**
tuple payload: payload (Beskid.Syntax.Nodes.MemberExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Path` (`enum_variant`)



**Variant `Path`**
tuple payload: payload (Beskid.Syntax.Nodes.PathExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Spawn` (`enum_variant`)



**Variant `Spawn`**
tuple payload: payload (Beskid.Syntax.Nodes.SpawnExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::StructLiteral` (`enum_variant`)



**Variant `StructLiteral`**
tuple payload: payload (Beskid.Syntax.Nodes.StructLiteralExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Try` (`enum_variant`)



**Variant `Try`**
tuple payload: payload (Beskid.Syntax.Nodes.TryExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::Unary` (`enum_variant`)



**Variant `Unary`**
tuple payload: payload (Beskid.Syntax.Nodes.UnaryExpression).


---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Expression::Expression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExpressionList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExpressionStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExpressionStatement::ExpressionStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/expression_statement.rs` — `ExpressionStatement`.

**Rust documentation** (from mirrored type):
Statement that evaluates an expression for side effects (typically terminated with `;`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `expression`.

---

### `Beskid::Syntax::Nodes::ExpressionStatement::ExpressionStatement::expression` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExtendTypeDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/extend_type.rs` — `ExtendTypeDefinition`.

**Rust documentation** (from mirrored type):
`extend type T { ... }` block preserving source grouping for extension semantics.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `targetType`, `methods`.

---

### `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition::methods` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ExtendTypeDefinition::ExtendTypeDefinition::targetType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Field` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Field::Field` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/field.rs` — `Field`.

**Rust documentation** (from mirrored type):
Struct or enum variant field with name and type (and optional event capacity).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `name`, `ty`.

---

### `Beskid::Syntax::Nodes::Field::Field::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Field::Field::ty` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Field::Field::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FieldKind` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FieldKind::FieldKind` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/field.rs` — `FieldKind`.

**Rust documentation** (from mirrored type):
Distinguishes ordinary value fields from event/signal-style fields.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Value`**
unit (no payload)


**Variant `Event`**
unit (no payload)


**Variant `Injected`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::FieldKind::FieldKind::Event` (`enum_variant`)



**Variant `Event`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::FieldKind::FieldKind::Injected` (`enum_variant`)



**Variant `Injected`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::FieldKind::FieldKind::Value` (`enum_variant`)



**Variant `Value`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::FieldList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ForStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ForStatement::ForStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/for_statement.rs` — `ForStatement`.

**Rust documentation** (from mirrored type):
`for` loop over an iterable value.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `iterator`, `iterable`, `body`.

---

### `Beskid::Syntax::Nodes::ForStatement::ForStatement::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ForStatement::ForStatement::iterable` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ForStatement::ForStatement::iterator` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/function_definition.rs` — `FunctionDefinition`.

**Rust documentation** (from mirrored type):
Top-level or nested function: visibility, signature, and body block.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `attributes`, `visibility`, `name`, `generics`, `parameters`, `returnType`, `body`.

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::attributes` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::generics` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::returnType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::FunctionDefinition::FunctionDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::GroupedExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::GroupedExpression::GroupedExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/grouped_expression.rs` — `GroupedExpression`.

**Rust documentation** (from mirrored type):
Parenthesized subexpression (grouping / precedence).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `expr`.

---

### `Beskid::Syntax::Nodes::GroupedExpression::GroupedExpression::expr` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostBodyItem` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostBodyItemList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostDefinition::HostDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `HostDefinition`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `parameters`, `baseHost`, `body`.

---

### `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::baseHost` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::HostDefinition::HostDefinition::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Identifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Identifier::Identifier` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/common/identifier.rs` — `Identifier`.

**Rust documentation** (from mirrored type):
Unqualified identifier as parsed from source (name text only).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`.

---

### `Beskid::Syntax::Nodes::Identifier::Identifier::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IdentifierList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IfStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IfStatement::IfStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/if_statement.rs` — `IfStatement`.

**Rust documentation** (from mirrored type):
Conditional with mandatory then-block and optional `else` block.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `condition`, `thenBlock`, `elseBlock`.

---

### `Beskid::Syntax::Nodes::IfStatement::IfStatement::condition` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IfStatement::IfStatement::elseBlock` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IfStatement::IfStatement::thenBlock` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IndexExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IndexExpression::IndexExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/index_expression.rs` — `IndexExpression`.

**Rust documentation** (from mirrored type):
`expr[index]` — array/string element access.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `target`, `index`.

---

### `Beskid::Syntax::Nodes::IndexExpression::IndexExpression::index` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::IndexExpression::IndexExpression::target` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InjectQualifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `InjectQualifier`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Global`**
unit (no payload)


**Variant `Parent`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier::Global` (`enum_variant`)



**Variant `Global`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::InjectQualifier::InjectQualifier::Parent` (`enum_variant`)



**Variant `Parent`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::InlineModule` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InlineModule::InlineModule` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/inline_module.rs` — `InlineModule`.

**Rust documentation** (from mirrored type):
Inline `module Name { ... }` with nested items and optional leading docs per item.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `attributes`, `visibility`, `name`, `items`.

---

### `Beskid::Syntax::Nodes::InlineModule::InlineModule::attributes` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InlineModule::InlineModule::items` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InlineModule::InlineModule::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::InlineModule::InlineModule::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/lambda_expression.rs` — `LambdaExpression`.

**Rust documentation** (from mirrored type):
Anonymous function expression (`params => body`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `parameters`, `body`.

---

### `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaExpression::LambdaExpression::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaParameter` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/lambda_expression.rs` — `LambdaParameter`.

**Rust documentation** (from mirrored type):
Single lambda parameter, optionally with an explicit type.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `ty`.

---

### `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaParameter::LambdaParameter::ty` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LambdaParameterList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LaunchStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `LaunchStatement`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `hostPath`, `arguments`.

---

### `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement::arguments` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LaunchStatement::LaunchStatement::hostPath` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LetStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LetStatement::LetStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/let_statement.rs` — `LetStatement`.

**Rust documentation** (from mirrored type):
Local binding with optional type annotation and mandatory initializer.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `typeAnnotation`, `value`.

---

### `Beskid::Syntax::Nodes::LetStatement::LetStatement::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LetStatement::LetStatement::typeAnnotation` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LetStatement::LetStatement::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal::Literal` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/literal.rs` — `Literal`.

**Rust documentation** (from mirrored type):
Literal token; numeric and text forms keep raw source text where precision matters.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Integer`**
tuple (payload: string)


**Variant `Float`**
tuple (payload: string)


**Variant `String`**
tuple (payload: string)


**Variant `Char`**
tuple (payload: string)


**Variant `Bool`**
tuple (payload: bool)


---

### `Beskid::Syntax::Nodes::Literal::Literal::Bool` (`enum_variant`)



**Variant `Bool`**
tuple payload: payload (bool).


---

### `Beskid::Syntax::Nodes::Literal::Literal::Char` (`enum_variant`)



**Variant `Char`**
tuple payload: payload (string).


---

### `Beskid::Syntax::Nodes::Literal::Literal::Float` (`enum_variant`)



**Variant `Float`**
tuple payload: payload (string).


---

### `Beskid::Syntax::Nodes::Literal::Literal::Integer` (`enum_variant`)



**Variant `Integer`**
tuple payload: payload (string).


---

### `Beskid::Syntax::Nodes::Literal::Literal::String` (`enum_variant`)



**Variant `String`**
tuple payload: payload (string).


---

### `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Literal::Literal::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LiteralExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::LiteralExpression::LiteralExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/literal_expression.rs` — `LiteralExpression`.

**Rust documentation** (from mirrored type):
Expression consisting of a single [`Literal`]; string literals may desugar to concatenation.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `literal`.

---

### `Beskid::Syntax::Nodes::LiteralExpression::LiteralExpression::literal` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/macro_definition.rs` — `MacroDefinition`.

**Rust documentation** (from mirrored type):
`macro name (kind param, ...) { body }` module item.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `name`, `parameters`, `body`.

---

### `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroDefinition::MacroDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroFragmentKind` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/macro_definition.rs` — `MacroFragmentKind`.

**Rust documentation** (from mirrored type):
Fragment kind for a macro parameter (`block`, `expression`, …).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Block`**
unit (no payload)


**Variant `Expression`**
unit (no payload)


**Variant `Statement`**
unit (no payload)


**Variant `Type`**
unit (no payload)


**Variant `Identifier`**
unit (no payload)


**Variant `Literal`**
unit (no payload)


**Variant `Pattern`**
unit (no payload)


**Variant `Path`**
unit (no payload)


**Variant `Item`**
unit (no payload)


**Variant `Node`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Block` (`enum_variant`)



**Variant `Block`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Expression` (`enum_variant`)



**Variant `Expression`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Identifier` (`enum_variant`)



**Variant `Identifier`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Item` (`enum_variant`)



**Variant `Item`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Literal` (`enum_variant`)



**Variant `Literal`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Node` (`enum_variant`)



**Variant `Node`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Path` (`enum_variant`)



**Variant `Path`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Pattern` (`enum_variant`)



**Variant `Pattern`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Statement` (`enum_variant`)



**Variant `Statement`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroFragmentKind::MacroFragmentKind::Type` (`enum_variant`)



**Variant `Type`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::MacroInvocation` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/macro_invocation.rs` — `MacroInvocation`.

**Rust documentation** (from mirrored type):
`name!(args)` / `name! { block }` macro invocation expression.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `arguments`, `block`.

---

### `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::arguments` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::block` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroInvocation::MacroInvocation::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroMetavariable` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroMetavariable::MacroMetavariable` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/macro_metavariable.rs` — `MacroMetavariable`.

**Rust documentation** (from mirrored type):
`$name` reference inside a macro definition body.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`.

---

### `Beskid::Syntax::Nodes::MacroMetavariable::MacroMetavariable::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroParameter` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroParameter::MacroParameter` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/macro_definition.rs` — `MacroParameter`.

**Rust documentation** (from mirrored type):
One formal parameter in a `macro` definition.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `kind`, `name`.

---

### `Beskid::Syntax::Nodes::MacroParameter::MacroParameter::kind` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroParameter::MacroParameter::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MacroParameterList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchArm` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchArm::MatchArm` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/match_arm.rs` — `MatchArm`.

**Rust documentation** (from mirrored type):
One `pattern [if guard] => expr` arm in a [`MatchExpression`](super::MatchExpression).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `pattern`, `guard`, `value`.

---

### `Beskid::Syntax::Nodes::MatchArm::MatchArm::guard` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchArm::MatchArm::pattern` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchArm::MatchArm::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchArmList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchExpression::MatchExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/match_expression.rs` — `MatchExpression`.

**Rust documentation** (from mirrored type):
`match` expression: scrutinee and ordered arms.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `scrutinee`, `arms`.

---

### `Beskid::Syntax::Nodes::MatchExpression::MatchExpression::arms` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MatchExpression::MatchExpression::scrutinee` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MemberExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MemberExpression::MemberExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/member_expression.rs` — `MemberExpression`.

**Rust documentation** (from mirrored type):
Field or member access (`expr.member`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `target`, `member`.

---

### `Beskid::Syntax::Nodes::MemberExpression::MemberExpression::member` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MemberExpression::MemberExpression::target` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/method_definition.rs` — `MethodDefinition`.

**Rust documentation** (from mirrored type):
Method inside an `impl` block: receiver type, parameters, return type, and body.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `receiverType`, `name`, `parameters`, `returnType`, `body`.

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::receiverType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::returnType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinition::MethodDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::MethodDefinitionList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ModuleDeclaration` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/module_declaration.rs` — `ModuleDeclaration`.

**Rust documentation** (from mirrored type):
Out-of-line module declaration (`module path;`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `attributes`, `visibility`, `path`.

---

### `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::attributes` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ModuleDeclaration::ModuleDeclaration::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::Node` (`contract`)

Sole navigation/query contract for syntax nodes in Mod SDK code.

---

### `Beskid::Syntax::Nodes::Node::Node::Kind` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::Node::PushChildren` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::Node::Ref` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::Node::Span` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::Node::sink` (`parameter`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::NodeChildSink` (`contract`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::NodeChildSink::Push` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Node::NodeChildSink::child` (`parameter`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind` (`enum`)

Classification tokens for syntax query (mirrors `beskid_analysis::query::NodeKind`).

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::AssignExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Attribute` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeArgument` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeDeclaration` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeParameter` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::AttributeTarget` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::BinaryExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::BinaryOp` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Block` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::BlockExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::BreakStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::CallExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContinueStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractEmbedding` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractMethodSignature` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ContractNode` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumConstructorExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumPath` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumPattern` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::EnumVariant` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Expression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ExpressionStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ExtendTypeDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Field` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ForStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::FunctionDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::GroupedExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::HostBodyItem` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::HostDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Identifier` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::IfStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::InlineModule` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::LambdaExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::LambdaParameter` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::LaunchStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::LetStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Literal` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::LiteralExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroFragmentKind` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroInvocation` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroMetavariable` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MacroParameter` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MatchArm` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MatchExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MemberExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::MethodDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ModuleDeclaration` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Node` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Parameter` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ParameterModifier` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Path` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::PathExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::PathSegment` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Pattern` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::PrimitiveType` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Program` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::RangeExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::RegistryBlock` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::RegistryEntry` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ReturnStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ScopeDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::ScopeHook` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::SpawnExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Statement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::StructLiteralExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::StructLiteralField` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestMetaSection` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestMetadataEntry` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestSkipEntry` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TestSkipSection` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TryExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Type` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::TypeDefinition` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::UnaryExpression` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::UnaryOp` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::UseDeclaration` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::Visibility` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::WhileStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeKind::NodeKind::WithStatement` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeRef` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeRef::NodeRef` (`type`)

Opaque stable handle for a syntax node within one `syntaxGenerationId` window.

---

### `Beskid::Syntax::Nodes::NodeRef::NodeRef::nodeId` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeRef::NodeRef::syntaxGenerationId` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan` (`type`)

Source span for one syntax node in one generation.

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::columnEnd` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::columnStart` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::end` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::lineEnd` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::lineStart` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::NodeSpan::NodeSpan::start` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalBlock` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.Block` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.Block`).


---

### `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalBlock::OptionalBlock::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.Expression` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.Expression`).


---

### `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalExpression::OptionalExpression::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalIdentifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.Identifier` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.Identifier`).


---

### `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalIdentifier::OptionalIdentifier::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalInjectQualifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.InjectQualifier` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.InjectQualifier`).


---

### `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalInjectQualifier::OptionalInjectQualifier::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalLeadingDocComment` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.LeadingDocComment` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.LeadingDocComment`).


---

### `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalLeadingDocComment::OptionalLeadingDocComment::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalParameterModifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.ParameterModifier` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.ParameterModifier`).


---

### `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalParameterModifier::OptionalParameterModifier::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalPath` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalPath::OptionalPath` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.Path` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.Path`).


---

### `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalPath::OptionalPath::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalRegistrationLifetime` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.RegistrationLifetime` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.RegistrationLifetime`).


---

### `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalRegistrationLifetime::OptionalRegistrationLifetime::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestMetaSection` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.TestMetaSection` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.TestMetaSection`).


---

### `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestMetaSection::OptionalTestMetaSection::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestSkipSection` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.TestSkipSection` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.TestSkipSection`).


---

### `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalTestSkipSection::OptionalTestSkipSection::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalType` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalType::OptionalType` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.Type` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.Type`).


---

### `Beskid::Syntax::Nodes::OptionalType::OptionalType::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalType::OptionalType::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::OptionalType::OptionalType::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Optionalusize` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Optionalusize::Optionalusize` (`enum`)

Rust `Option<…>` encoding where the inner type is `Beskid.Syntax.Nodes.usize` (`beskid_doc.pest` `@variant`).

**Variant `None`**
Absent (`None` in Rust).


**Variant `Some`**
Present; `payload` holds the inner value (`Beskid.Syntax.Nodes.usize`).


---

### `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::None` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::Some` (`enum_variant`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Optionalusize::Optionalusize::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Parameter` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Parameter::Parameter` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/parameter.rs` — `Parameter`.

**Rust documentation** (from mirrored type):
Function or method parameter: optional modifier, name, and type (`ty name` surface order).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `modifier`, `name`, `ty`.

---

### `Beskid::Syntax::Nodes::Parameter::Parameter::modifier` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Parameter::Parameter::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Parameter::Parameter::ty` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ParameterList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ParameterModifier` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/parameter_modifier.rs` — `ParameterModifier`.

**Rust documentation** (from mirrored type):
`ref` or `out` parameter modifier.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Ref`**
unit (no payload)


**Variant `Out`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier::Out` (`enum_variant`)



**Variant `Out`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::ParameterModifier::ParameterModifier::Ref` (`enum_variant`)



**Variant `Ref`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Path` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Path::Path` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/path.rs` — `Path`.

**Rust documentation** (from mirrored type):
Qualified name path (`a.b.C`) used in types and expressions.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `segments`.

---

### `Beskid::Syntax::Nodes::Path::Path::segments` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathExpression::PathExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/path_expression.rs` — `PathExpression`.

**Rust documentation** (from mirrored type):
Path used as a value expression (name resolution happens later).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `path`.

---

### `Beskid::Syntax::Nodes::PathExpression::PathExpression::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathSegment` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathSegment::PathSegment` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/path.rs` — `PathSegment`.

**Rust documentation** (from mirrored type):
One segment of a dotted path, with optional generic type arguments.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `typeArgs`.

---

### `Beskid::Syntax::Nodes::PathSegment::PathSegment::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathSegment::PathSegment::typeArgs` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PathSegmentList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Pattern` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Pattern::Pattern` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/pattern.rs` — `Pattern`.

**Rust documentation** (from mirrored type):
Match pattern: wildcard, binding, literal, or enum destructure.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Wildcard`**
unit (no payload)


**Variant `Identifier`**
tuple (payload: Beskid.Syntax.Nodes.Identifier)


**Variant `Literal`**
tuple (payload: Beskid.Syntax.Nodes.Literal)


**Variant `Enum`**
tuple (payload: Beskid.Syntax.Nodes.EnumPattern)


---

### `Beskid::Syntax::Nodes::Pattern::Pattern::Enum` (`enum_variant`)



**Variant `Enum`**
tuple payload: payload (Beskid.Syntax.Nodes.EnumPattern).


---

### `Beskid::Syntax::Nodes::Pattern::Pattern::Identifier` (`enum_variant`)



**Variant `Identifier`**
tuple payload: payload (Beskid.Syntax.Nodes.Identifier).


---

### `Beskid::Syntax::Nodes::Pattern::Pattern::Literal` (`enum_variant`)



**Variant `Literal`**
tuple payload: payload (Beskid.Syntax.Nodes.Literal).


---

### `Beskid::Syntax::Nodes::Pattern::Pattern::Wildcard` (`enum_variant`)



**Variant `Wildcard`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Pattern::Pattern::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PatternList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PrimitiveType` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/primitive_type.rs` — `PrimitiveType`.

**Rust documentation** (from mirrored type):
Core primitive types supported in the surface language.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Bool`**
unit (no payload)


**Variant `I32`**
unit (no payload)


**Variant `I64`**
unit (no payload)


**Variant `U8`**
unit (no payload)


**Variant `F64`**
unit (no payload)


**Variant `Char`**
unit (no payload)


**Variant `String`**
unit (no payload)


**Variant `Unit`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Bool` (`enum_variant`)



**Variant `Bool`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Char` (`enum_variant`)



**Variant `Char`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::F64` (`enum_variant`)



**Variant `F64`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::I32` (`enum_variant`)



**Variant `I32`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::I64` (`enum_variant`)



**Variant `I64`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::String` (`enum_variant`)



**Variant `String`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::U8` (`enum_variant`)



**Variant `U8`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::PrimitiveType::PrimitiveType::Unit` (`enum_variant`)



**Variant `Unit`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Program` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Program::Program` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/program.rs` — `Program`.

**Rust documentation** (from mirrored type):
Parsed compilation unit: top-level items with optional leading doc comments per item.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `items`.

---

### `Beskid::Syntax::Nodes::Program::Program::items` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RangeExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RangeExpression::RangeExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/range_expression.rs` — `RangeExpression`.

**Rust documentation** (from mirrored type):
Inclusive-style range used in `for` headers (`start..end`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `start`, `end`.

---

### `Beskid::Syntax::Nodes::RangeExpression::RangeExpression::end` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RangeExpression::RangeExpression::start` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistrationLifetime` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `RegistrationLifetime`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Single`**
unit (no payload)


**Variant `Transient`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime::Single` (`enum_variant`)



**Variant `Single`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::RegistrationLifetime::RegistrationLifetime::Transient` (`enum_variant`)



**Variant `Transient`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::RegistryBlock` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistryBlock::RegistryBlock` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `RegistryBlock`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `entries`.

---

### `Beskid::Syntax::Nodes::RegistryBlock::RegistryBlock::entries` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistryEntry` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `RegistryEntry`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `implementation`, `target`.

---

### `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry::implementation` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistryEntry::RegistryEntry::target` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::RegistryEntryList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ReturnStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ReturnStatement::ReturnStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/return_statement.rs` — `ReturnStatement`.

**Rust documentation** (from mirrored type):
`return` with an optional value expression.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `value`.

---

### `Beskid::Syntax::Nodes::ReturnStatement::ReturnStatement::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `ScopeDefinition`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `parameters`, `body`.

---

### `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeDefinition::ScopeDefinition::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeHook` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeHook::ScopeHook` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `ScopeHook`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `parameters`, `body`.

---

### `Beskid::Syntax::Nodes::ScopeHook::ScopeHook::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeHook::ScopeHook::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeHookKind` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `ScopeHookKind`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Init`**
unit (no payload)


**Variant `Dispose`**
unit (no payload)


**Variant `Startup`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Dispose` (`enum_variant`)



**Variant `Dispose`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Init` (`enum_variant`)



**Variant `Init`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::ScopeHookKind::ScopeHookKind::Startup` (`enum_variant`)



**Variant `Startup`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::SpawnExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::SpawnExpression::SpawnExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/spawn_expression.rs` — `SpawnExpression`.

**Rust documentation** (from mirrored type):
`spawn` prefix expression: starts a new fiber from a callable operand.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `callee`.

---

### `Beskid::Syntax::Nodes::SpawnExpression::SpawnExpression::callee` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/statement.rs` — `Statement`.

**Rust documentation** (from mirrored type):
Executable statement inside a block (not a top-level item).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Let`**
tuple (payload: Beskid.Syntax.Nodes.LetStatement)


**Variant `Return`**
tuple (payload: Beskid.Syntax.Nodes.ReturnStatement)


**Variant `Break`**
tuple (payload: Beskid.Syntax.Nodes.BreakStatement)


**Variant `Continue`**
tuple (payload: Beskid.Syntax.Nodes.ContinueStatement)


**Variant `While`**
tuple (payload: Beskid.Syntax.Nodes.WhileStatement)


**Variant `For`**
tuple (payload: Beskid.Syntax.Nodes.ForStatement)


**Variant `If`**
tuple (payload: Beskid.Syntax.Nodes.IfStatement)


**Variant `With`**
tuple (payload: Beskid.Syntax.Nodes.WithStatement)


**Variant `Launch`**
tuple (payload: Beskid.Syntax.Nodes.LaunchStatement)


**Variant `Expression`**
tuple (payload: Beskid.Syntax.Nodes.ExpressionStatement)


---

### `Beskid::Syntax::Nodes::Statement::Statement::Break` (`enum_variant`)



**Variant `Break`**
tuple payload: payload (Beskid.Syntax.Nodes.BreakStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::Continue` (`enum_variant`)



**Variant `Continue`**
tuple payload: payload (Beskid.Syntax.Nodes.ContinueStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::Expression` (`enum_variant`)



**Variant `Expression`**
tuple payload: payload (Beskid.Syntax.Nodes.ExpressionStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::For` (`enum_variant`)



**Variant `For`**
tuple payload: payload (Beskid.Syntax.Nodes.ForStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::If` (`enum_variant`)



**Variant `If`**
tuple payload: payload (Beskid.Syntax.Nodes.IfStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::Launch` (`enum_variant`)



**Variant `Launch`**
tuple payload: payload (Beskid.Syntax.Nodes.LaunchStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::Let` (`enum_variant`)



**Variant `Let`**
tuple payload: payload (Beskid.Syntax.Nodes.LetStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::Return` (`enum_variant`)



**Variant `Return`**
tuple payload: payload (Beskid.Syntax.Nodes.ReturnStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::While` (`enum_variant`)



**Variant `While`**
tuple payload: payload (Beskid.Syntax.Nodes.WhileStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::With` (`enum_variant`)



**Variant `With`**
tuple payload: payload (Beskid.Syntax.Nodes.WithStatement).


---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Statement::Statement::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StatementList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/struct_literal_expression.rs` — `StructLiteralExpression`.

**Rust documentation** (from mirrored type):
Struct or nominal value literal: path plus field assignments.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `path`, `fields`.

---

### `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression::fields` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralExpression::StructLiteralExpression::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralField` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/struct_literal_field.rs` — `StructLiteralField`.

**Rust documentation** (from mirrored type):
Single `name: value` field in a struct literal.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `value`.

---

### `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralField::StructLiteralField::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::StructLiteralFieldList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/test_definition.rs` — `TestDefinition`.

**Rust documentation** (from mirrored type):
`test` item: optional meta/skip sections and a statement body with optional statement docs.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `attributes`, `visibility`, `name`, `_meta`, `_skip`, `statements`.

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::_meta` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::_skip` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::attributes` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::statements` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestDefinition::TestDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetaSection` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetaSection::TestMetaSection` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/test_definition.rs` — `TestMetaSection`.

**Rust documentation** (from mirrored type):
Braced `meta` section inside a test body.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `entries`.

---

### `Beskid::Syntax::Nodes::TestMetaSection::TestMetaSection::entries` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetadataEntry` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/test_definition.rs` — `TestMetadataEntry`.

**Rust documentation** (from mirrored type):
Single `name = expr` entry in a test `meta { ... }` section.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `value`.

---

### `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetadataEntry::TestMetadataEntry::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestMetadataEntryList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipEntry` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/test_definition.rs` — `TestSkipEntry`.

**Rust documentation** (from mirrored type):
Entry in a test `skip { ... }` section (conditional skip metadata).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `name`, `value`.

---

### `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipEntry::TestSkipEntry::value` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipEntryList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipSection` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TestSkipSection::TestSkipSection` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/test_definition.rs` — `TestSkipSection`.

**Rust documentation** (from mirrored type):
Braced `skip` section inside a test body.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `entries`.

---

### `Beskid::Syntax::Nodes::TestSkipSection::TestSkipSection::entries` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TraversalManifest` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TryExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TryExpression::TryExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/try_expression.rs` — `TryExpression`.

**Rust documentation** (from mirrored type):
`expr?` — propagating try operator applied to an inner expression.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `expr`.

---

### `Beskid::Syntax::Nodes::TryExpression::TryExpression::expr` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/types/type.rs` — `Type`.

**Rust documentation** (from mirrored type):
Beskid type expression: primitives, paths, arrays, references, and function types.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Primitive`**
tuple (payload: Beskid.Syntax.Nodes.PrimitiveType)


**Variant `Complex`**
tuple (payload: Beskid.Syntax.Nodes.Path)


**Variant `Array`**
tuple (payload: Beskid.Syntax.Nodes.Type)


**Variant `Ref`**
tuple (payload: Beskid.Syntax.Nodes.Type)


**Variant `Function`**
struct { returnType: Beskid.Syntax.Nodes.Type, parameters: Beskid.Syntax.Nodes.TypeList }


---

### `Beskid::Syntax::Nodes::Type::Type::Array` (`enum_variant`)



**Variant `Array`**
tuple payload: payload (Beskid.Syntax.Nodes.Type).


---

### `Beskid::Syntax::Nodes::Type::Type::Complex` (`enum_variant`)



**Variant `Complex`**
tuple payload: payload (Beskid.Syntax.Nodes.Path).


---

### `Beskid::Syntax::Nodes::Type::Type::Function` (`enum_variant`)



**Variant `Function`**
struct payload: returnType: Beskid.Syntax.Nodes.Type, parameters: Beskid.Syntax.Nodes.TypeList.


---

### `Beskid::Syntax::Nodes::Type::Type::Primitive` (`enum_variant`)



**Variant `Primitive`**
tuple payload: payload (Beskid.Syntax.Nodes.PrimitiveType).


---

### `Beskid::Syntax::Nodes::Type::Type::Ref` (`enum_variant`)



**Variant `Ref`**
tuple payload: payload (Beskid.Syntax.Nodes.Type).


---

### `Beskid::Syntax::Nodes::Type::Type::parameters` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type::payload` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Type::Type::returnType` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/type_definition.rs` — `TypeDefinition`.

**Rust documentation** (from mirrored type):
`type` definition: name, generics, optional conformances, and fields.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `name`, `generics`, `conformances`, `fields`.

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::conformances` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::fields` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::generics` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::name` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeDefinition::TypeDefinition::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::TypeList` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UnaryExpression` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/unary_expression.rs` — `UnaryExpression`.

**Rust documentation** (from mirrored type):
Unary prefix operator applied to an operand.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `op`, `expr`.

---

### `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression::expr` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UnaryExpression::UnaryExpression::op` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UnaryOp` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UnaryOp::UnaryOp` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/expressions/unary_expression.rs` — `UnaryOp`.

**Rust documentation** (from mirrored type):
Supported unary operators (`-`, `!`).

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Neg`**
unit (no payload)


**Variant `Not`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::UnaryOp::UnaryOp::Neg` (`enum_variant`)



**Variant `Neg`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::UnaryOp::UnaryOp::Not` (`enum_variant`)



**Variant `Not`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::UseDeclaration` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/use_declaration.rs` — `UseDeclaration`.

**Rust documentation** (from mirrored type):
`use` import: path with optional alias.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `visibility`, `path`, `alias`.

---

### `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::alias` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::path` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::UseDeclaration::UseDeclaration::visibility` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visibility` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visibility::Visibility` (`enum`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/common/visibility.rs` — `Visibility`.

**Rust documentation** (from mirrored type):
Visibility applied to a module item or attribute declaration.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.

**Variant `Public`**
unit (no payload)


**Variant `Private`**
unit (no payload)


---

### `Beskid::Syntax::Nodes::Visibility::Visibility::Private` (`enum_variant`)



**Variant `Private`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Visibility::Visibility::Public` (`enum_variant`)



**Variant `Public`**
unit variant (no payload).


---

### `Beskid::Syntax::Nodes::Visit` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visit::SyntaxVisitor` (`contract`)

Depth-first visitor contract (lowers to `beskid_analysis::query::AstWalker` / `Visit`).

---

### `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::Enter` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::Exit` (`contract_method`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::node` (`parameter`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::Visit::SyntaxVisitor::node` (`parameter`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WhileStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WhileStatement::WhileStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/statements/while_statement.rs` — `WhileStatement`.

**Rust documentation** (from mirrored type):
`while` loop: condition evaluated before each iteration.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `condition`, `body`.

---

### `Beskid::Syntax::Nodes::WhileStatement::WhileStatement::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WhileStatement::WhileStatement::condition` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WithStatement` (`module`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WithStatement::WithStatement` (`type`)

Generated syntax node mirror: `crates/beskid_analysis/src/syntax/items/host_definition.rs` — `WithStatement`.

Generated index uses `beskid_doc.pest` `@variant` / `@par`; struct types list field names once (`@arg` is for callables only).
Implements `Node` via the host bridge; traverse with `Beskid.Compiler.Query` and `NodeRef`.
Struct fields (see declaration): `scopeName`, `arguments`, `body`.

---

### `Beskid::Syntax::Nodes::WithStatement::WithStatement::arguments` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WithStatement::WithStatement::body` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::Nodes::WithStatement::WithStatement::scopeName` (`field`)

*No documentation provided.*

---

### `Beskid::Syntax::SyntaxFacadeVersion` (`function`)

*No documentation provided.*

---

### `Collections::Array` (`module`)

Re-exports growable array surface.

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

### `Concurrency` (`module`)

Concurrency package prelude: cooperative fibers, channels, and OS threading.

---

### `Concurrency::Channel` (`module`)

*No documentation provided.*

---

### `Concurrency::Channel::Channel` (`type`)

Opaque channel handle (move-only at language layer).

---

### `Concurrency::Channel::Channel::handle` (`field`)

*No documentation provided.*

---

### `Concurrency::Channel::Close` (`function`)

Idempotent writer shutdown.

---

### `Concurrency::Channel::Close::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Channel::Create` (`function`)

Creates an unbounded channel (default per platform spec).

---

### `Concurrency::Channel::CreateWithOptions` (`function`)

Creates a channel with explicit options.

---

### `Concurrency::Channel::CreateWithOptions::options` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Channel::Handle` (`function`)

Runtime queue id for `Hub.Register` and builtins.

---

### `Concurrency::Channel::Handle::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Channel::MapReceiveError` (`function`)

*No documentation provided.*

---

### `Concurrency::Channel::MapReceiveError::status` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Channel::MapSendStatus` (`function`)

*No documentation provided.*

---

### `Concurrency::Channel::MapSendStatus::status` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Channel::Receive` (`function`)

Receives a value; parks when empty and open.

---

### `Concurrency::Channel::Receive::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Channel::Send` (`function`)

Sends a value; parks the current fiber when the queue is full (bounded + wait mode).

---

### `Concurrency::Channel::Send::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Channel::Send::value` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Channel::SendOk` (`type`)

Success marker for [`Send`] / [`TrySend`] (`Result<SendOk, ChannelError>` per platform spec `Result<(), _>`).

---

### `Concurrency::Channel::TryReceive` (`function`)

Non-blocking receive; `None` when the operation would block.

---

### `Concurrency::Channel::TryReceive::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Channel::TrySend` (`function`)

Non-blocking send; `None` when the operation would block.

---

### `Concurrency::Channel::TrySend::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Channel::TrySend::value` (`parameter`)

*No documentation provided.*

---

### `Concurrency::ChannelError` (`module`)

*No documentation provided.*

---

### `Concurrency::ChannelError::ChannelError` (`enum`)

Errors for blocking channel operations (`Send`, `Receive`).

**Variant `Closed`**
Endpoint closed.


**Variant `Cancelled`**
Owning fiber was cancelled.


---

### `Concurrency::ChannelError::ChannelError::Cancelled` (`enum_variant`)

*No documentation provided.*

---

### `Concurrency::ChannelError::ChannelError::Closed` (`enum_variant`)

*No documentation provided.*

---

### `Concurrency::ChannelOptions` (`module`)

*No documentation provided.*

---

### `Concurrency::ChannelOptions::Bounded` (`function`)

Bounded queue with `capacity > 0`; senders park when full.

---

### `Concurrency::ChannelOptions::Bounded::capacity` (`parameter`)

*No documentation provided.*

---

### `Concurrency::ChannelOptions::ChannelCapacity` (`enum`)

Construction options for `Channel<T>.Create`.
Capacity selector: default is unbounded.

---

### `Concurrency::ChannelOptions::ChannelCapacity::Bounded` (`enum_variant`)

*No documentation provided.*

---

### `Concurrency::ChannelOptions::ChannelCapacity::Unbounded` (`enum_variant`)

*No documentation provided.*

---

### `Concurrency::ChannelOptions::ChannelCapacity::capacity` (`field`)

*No documentation provided.*

---

### `Concurrency::ChannelOptions::ChannelOptions` (`type`)

Channel factory options (capacity plus v1 optimization hints).

---

### `Concurrency::ChannelOptions::ChannelOptions::capacity` (`field`)

*No documentation provided.*

---

### `Concurrency::ChannelOptions::ChannelOptions::singleReader` (`field`)

*No documentation provided.*

---

### `Concurrency::ChannelOptions::ChannelOptions::singleWriter` (`field`)

*No documentation provided.*

---

### `Concurrency::ChannelOptions::Default` (`function`)

Default options: unbounded queue, no single-reader/writer hints.

---

### `Concurrency::ChannelOptions::EncodeCapacity` (`function`)

Encodes capacity for `__channel_create` (`0` = unbounded).

---

### `Concurrency::ChannelOptions::EncodeCapacity::options` (`parameter`)

*No documentation provided.*

---

### `Concurrency::ChannelOptions::EncodeFlags` (`function`)

Encodes reader/writer hint flags for `__channel_create`.

---

### `Concurrency::ChannelOptions::EncodeFlags::options` (`parameter`)

*No documentation provided.*

---

### `Concurrency::ChannelOptions::SingleReader` (`function`)

Unbounded queue optimized for a single reader.

---

### `Concurrency::ChannelOptions::SingleWriter` (`function`)

Unbounded queue optimized for a single writer.

---

### `Concurrency::ChannelOptions::Unbounded` (`function`)

Explicit unbounded queue (equivalent to default).

---

### `Concurrency::Fiber` (`module`)

*No documentation provided.*

---

### `Concurrency::Fiber::Cancel` (`function`)

Sets cancellation flag and raises `OnCancelled` on the child fiber.

---

### `Concurrency::Fiber::Cancel::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Fiber::Detach` (`function`)

Waives parent shutdown join; child panic still aborts the process in v1.

---

### `Concurrency::Fiber::Detach::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Fiber::Fiber` (`type`)

Runtime fiber handle; declares cancellation event on the handle only.

---

### `Concurrency::Fiber::Fiber::OnCancelled` (`field`)

*No documentation provided.*

---

### `Concurrency::Fiber::Fiber::handle` (`field`)

*No documentation provided.*

---

### `Concurrency::Fiber::Join` (`function`)

Waits for completion and returns the entry value or a `FiberError`.

---

### `Concurrency::Fiber::Join::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Fiber::MapJoinError` (`function`)

*No documentation provided.*

---

### `Concurrency::Fiber::MapJoinError::status` (`parameter`)

*No documentation provided.*

---

### `Concurrency::FiberError` (`module`)

*No documentation provided.*

---

### `Concurrency::FiberError::FiberError` (`enum`)

Errors returned from `Fiber.Join`.

**Variant `Cancelled`**
Fiber was cancelled after `Cancel`.


**Variant `StackOverflow`**
Child exceeded stack growth policy.


**Variant `Panicked`**
Child exited via panic.


---

### `Concurrency::FiberError::FiberError::Cancelled` (`enum_variant`)

*No documentation provided.*

---

### `Concurrency::FiberError::FiberError::Panicked` (`enum_variant`)

*No documentation provided.*

---

### `Concurrency::FiberError::FiberError::StackOverflow` (`enum_variant`)

*No documentation provided.*

---

### `Concurrency::FiberError::FiberError::code` (`field`)

*No documentation provided.*

---

### `Concurrency::FiberJoinStatus` (`module`)

*No documentation provided.*

---

### `Concurrency::FiberJoinStatus::Cancelled` (`function`)

*No documentation provided.*

---

### `Concurrency::FiberJoinStatus::NotDone` (`function`)

*No documentation provided.*

---

### `Concurrency::FiberJoinStatus::Ok` (`function`)

ABI status codes returned by `__fiber_join`.

---

### `Concurrency::FiberJoinStatus::Panicked` (`function`)

*No documentation provided.*

---

### `Concurrency::FiberJoinStatus::StackOverflow` (`function`)

*No documentation provided.*

---

### `Concurrency::Hub` (`module`)

*No documentation provided.*

---

### `Concurrency::Hub::Create` (`function`)

*No documentation provided.*

---

### `Concurrency::Hub::Hub` (`type`)

Multiplexes `Channel<T>` members with `WaitReceive`.

---

### `Concurrency::Hub::Hub::handle` (`field`)

*No documentation provided.*

---

### `Concurrency::Hub::Register` (`function`)

Registers `channel` at `index` (max 256 members in v1).

---

### `Concurrency::Hub::Register::channel` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Hub::Register::index` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Hub::Register::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Hub::Unregister` (`function`)

*No documentation provided.*

---

### `Concurrency::Hub::Unregister::index` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Hub::Unregister::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Hub::WaitReceive` (`function`)

Blocks until a member can satisfy `Receive` (round-robin among ready channels).

---

### `Concurrency::Hub::WaitReceive::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::HubError` (`module`)

*No documentation provided.*

---

### `Concurrency::HubError::HubError` (`enum`)

Errors from `Hub.WaitReceive`.

**Variant `Limit`**
Registration cap exceeded (256 in v1).


**Variant `Cancelled`**
Waiting fiber was cancelled.


**Variant `Closed`**
All registered channels are closed.


---

### `Concurrency::HubError::HubError::Cancelled` (`enum_variant`)

*No documentation provided.*

---

### `Concurrency::HubError::HubError::Closed` (`enum_variant`)

*No documentation provided.*

---

### `Concurrency::HubError::HubError::Limit` (`enum_variant`)

*No documentation provided.*

---

### `Concurrency::HubReceiveResult` (`module`)

*No documentation provided.*

---

### `Concurrency::HubReceiveResult::HubReceiveResult` (`type`)

Successful `Hub.WaitReceive` payload.

---

### `Concurrency::HubReceiveResult::HubReceiveResult::index` (`field`)

User registration index passed to `Hub.Register`.

---

### `Concurrency::HubReceiveResult::HubReceiveResult::value` (`field`)

Message received from the selected member channel.

---

### `Concurrency::Mutex` (`module`)

*No documentation provided.*

---

### `Concurrency::Mutex::Create` (`function`)

*No documentation provided.*

---

### `Concurrency::Mutex::Lock` (`function`)

Parks until the mutex is acquired or the fiber is cancelled.

---

### `Concurrency::Mutex::Lock::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Mutex::Mutex` (`type`)

*No documentation provided.*

---

### `Concurrency::Mutex::Mutex::handle` (`field`)

*No documentation provided.*

---

### `Concurrency::Mutex::TryLock` (`function`)

Returns `None` when the mutex would block.

---

### `Concurrency::Mutex::TryLock::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Mutex::Unlock` (`function`)

*No documentation provided.*

---

### `Concurrency::Mutex::Unlock::guard` (`parameter`)

*No documentation provided.*

---

### `Concurrency::MutexError` (`module`)

*No documentation provided.*

---

### `Concurrency::MutexError::MutexError` (`enum`)

Errors from `Mutex.Lock`.

**Variant `Cancelled`**
Waiting fiber was cancelled before acquire.


---

### `Concurrency::MutexError::MutexError::Cancelled` (`enum_variant`)

*No documentation provided.*

---

### `Concurrency::MutexGuard` (`module`)

*No documentation provided.*

---

### `Concurrency::MutexGuard::MutexGuard` (`type`)

RAII guard acquired from `Mutex.Lock` or `Mutex.TryLock`.

---

### `Concurrency::MutexGuard::MutexGuard::mutexHandle` (`field`)

*No documentation provided.*

---

### `Concurrency::NowMillis` (`function`)

Monotonic milliseconds since an unspecified epoch (not wall clock).

---

### `Concurrency::ProcessorCount` (`function`)

Active scheduler worker count (defaults to host logical CPUs at init).

---

### `Concurrency::SetProcessorCount` (`function`)

Sets scheduler worker pool size (Go `GOMAXPROCS` analogue).
v1 Phase A: no-op until runtime exposes dynamic resizing.

---

### `Concurrency::SetProcessorCount::count` (`parameter`)

*No documentation provided.*

---

### `Concurrency::Status::Cancelled` (`function`)

*No documentation provided.*

---

### `Concurrency::Status::Closed` (`function`)

*No documentation provided.*

---

### `Concurrency::Status::HubEmpty` (`function`)

*No documentation provided.*

---

### `Concurrency::Status::HubLimit` (`function`)

*No documentation provided.*

---

### `Concurrency::Status::HubNotFound` (`function`)

*No documentation provided.*

---

### `Concurrency::Status::MutexBusy` (`function`)

*No documentation provided.*

---

### `Concurrency::Status::Ok` (`function`)

ABI status codes returned by channel, hub, mutex, and wait-group builtins.

---

### `Concurrency::Status::WouldBlock` (`function`)

*No documentation provided.*

---

### `Concurrency::WaitGroup` (`module`)

*No documentation provided.*

---

### `Concurrency::WaitGroup::Add` (`function`)

*No documentation provided.*

---

### `Concurrency::WaitGroup::Add::delta` (`parameter`)

*No documentation provided.*

---

### `Concurrency::WaitGroup::Add::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::WaitGroup::Create` (`function`)

*No documentation provided.*

---

### `Concurrency::WaitGroup::Done` (`function`)

*No documentation provided.*

---

### `Concurrency::WaitGroup::Done::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::WaitGroup::Wait` (`function`)

Parks until the counter reaches zero.

---

### `Concurrency::WaitGroup::Wait::self` (`parameter`)

*No documentation provided.*

---

### `Concurrency::WaitGroup::WaitGroup` (`type`)

Fork–join counter with `Add` / `Done` / `Wait` operations.

---

### `Concurrency::WaitGroup::WaitGroup::handle` (`field`)

*No documentation provided.*

---

### `Concurrency::Yield` (`function`)

Scheduler entry points (`fiber_yield`, monotonic clock, worker pool sizing).
Cooperative reschedule of the current fiber.

---

### `Console` (`module`)

Console package prelude.

---

### `Console::Capabilities` (`module`)

*No documentation provided.*

---

### `Console::Capabilities::Capabilities` (`type`)

*No documentation provided.*

---

### `Console::Capabilities::Capabilities::colorDisabled` (`field`)

*No documentation provided.*

---

### `Console::Capabilities::Capabilities::colorForced` (`field`)

*No documentation provided.*

---

### `Console::Capabilities::Capabilities::isTty` (`field`)

*No documentation provided.*

---

### `Console::Capabilities::Capabilities::model` (`field`)

*No documentation provided.*

---

### `Console::Capabilities::ColorModel` (`enum`)

*No documentation provided.*

---

### `Console::Capabilities::ColorModel::Basic16` (`enum_variant`)

*No documentation provided.*

---

### `Console::Capabilities::ColorModel::Basic8` (`enum_variant`)

*No documentation provided.*

---

### `Console::Capabilities::ColorModel::Indexed256` (`enum_variant`)

*No documentation provided.*

---

### `Console::Capabilities::ColorModel::TrueColor` (`enum_variant`)

*No documentation provided.*

---

### `Console::Capabilities::EffectiveColorModel` (`function`)

*No documentation provided.*

---

### `Console::Capabilities::EffectiveColorModel::caps` (`parameter`)

*No documentation provided.*

---

### `Console::Capabilities::IsStreamTty` (`function`)

*No documentation provided.*

---

### `Console::Capabilities::IsStreamTty::fd` (`parameter`)

*No documentation provided.*

---

### `Console::Capabilities::ProbeStdout` (`function`)

*No documentation provided.*

---

### `Console::Capabilities::ShouldEmitAnsi` (`function`)

*No documentation provided.*

---

### `Console::Capabilities::ShouldStripColor` (`function`)

*No documentation provided.*

---

### `Console::Capabilities::ShouldStripColor::caps` (`parameter`)

*No documentation provided.*

---

### `Console::ConsoleMessage` (`module`)

*No documentation provided.*

---

### `Console::ConsoleMessage::ConsoleMessage` (`enum`)

Cross-fiber terminal signals delivered via `Channel<ConsoleMessage>`.
Multiplexed console events for UI fibers (`Receive` / `Hub.WaitReceive`).

---

### `Console::ConsoleMessage::ConsoleMessage::Resize` (`enum_variant`)

Terminal size changed (character cells).

---

### `Console::ConsoleMessage::ConsoleMessage::Tick` (`enum_variant`)

Tick pulse for live controls (`Console.RunTick`).

---

### `Console::ConsoleMessage::ConsoleMessage::size` (`field`)

*No documentation provided.*

---

### `Console::ConsoleSize` (`type`)

Terminal dimensions in character cells.

---

### `Console::ConsoleSize::columns` (`field`)

*No documentation provided.*

---

### `Console::ConsoleSize::rows` (`field`)

*No documentation provided.*

---

### `Console::Controls::Contracts` (`module`)

*No documentation provided.*

---

### `Console::Controls::Contracts::ConsoleControl` (`contract`)

*No documentation provided.*

---

### `Console::Controls::Contracts::ConsoleControl::Measure` (`contract_method`)

*No documentation provided.*

---

### `Console::Controls::Contracts::ConsoleControl::Render` (`contract_method`)

*No documentation provided.*

---

### `Console::Controls::Contracts::ConsoleControl::available` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Contracts::ConsoleControl::size` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Contracts::Container` (`contract`)

*No documentation provided.*

---

### `Console::Controls::Contracts::Container::ChildCount` (`contract_method`)

*No documentation provided.*

---

### `Console::Controls::Contracts::FramedControl` (`contract`)

*No documentation provided.*

---

### `Console::Controls::Contracts::FramedControl::UseUnicodeFrame` (`contract_method`)

*No documentation provided.*

---

### `Console::Controls::Contracts::LiveControl` (`contract`)

Marker for controls driven by `Console.RunTick()`.
Implementing types should declare `event onTick();` on the type.

---

### `Console::Controls::Contracts::LiveControl::OnTick` (`contract_method`)

*No documentation provided.*

---

### `Console::Controls::Contracts::MarginProvider` (`contract`)

*No documentation provided.*

---

### `Console::Controls::Contracts::MarginProvider::Margin` (`contract_method`)

*No documentation provided.*

---

### `Console::Controls::Contracts::PaddingProvider` (`contract`)

*No documentation provided.*

---

### `Console::Controls::Contracts::PaddingProvider::Padding` (`contract_method`)

*No documentation provided.*

---

### `Console::Controls::Frame` (`module`)

*No documentation provided.*

---

### `Console::Controls::Frame::Ascii` (`function`)

*No documentation provided.*

---

### `Console::Controls::Frame::ForHost` (`function`)

*No documentation provided.*

---

### `Console::Controls::Frame::ForHost::useUnicode` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Frame::FrameChars` (`type`)

*No documentation provided.*

---

### `Console::Controls::Frame::FrameChars::bottom` (`field`)

*No documentation provided.*

---

### `Console::Controls::Frame::FrameChars::bottomLeft` (`field`)

*No documentation provided.*

---

### `Console::Controls::Frame::FrameChars::bottomRight` (`field`)

*No documentation provided.*

---

### `Console::Controls::Frame::FrameChars::side` (`field`)

*No documentation provided.*

---

### `Console::Controls::Frame::FrameChars::top` (`field`)

*No documentation provided.*

---

### `Console::Controls::Frame::FrameChars::topLeft` (`field`)

*No documentation provided.*

---

### `Console::Controls::Frame::FrameChars::topRight` (`field`)

*No documentation provided.*

---

### `Console::Controls::Frame::PreferUnicodeFrame` (`function`)

*No documentation provided.*

---

### `Console::Controls::Frame::Repeat` (`function`)

*No documentation provided.*

---

### `Console::Controls::Frame::Repeat::count` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Frame::Repeat::unit` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Frame::Unicode` (`function`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack` (`module`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::ChildCount` (`function`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::ChildCount::stack` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::HorizontalStack` (`type`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::HorizontalStack::childCount` (`field`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::HorizontalStack::segment` (`field`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::Measure` (`function`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::Measure::available` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::Measure::stack` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::New` (`function`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::Render` (`function`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::Render::size` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::Render::stack` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::RenderWithContext` (`function`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::RenderWithContext::ctx` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::RenderWithContext::size` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::RenderWithContext::stack` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::WithChild` (`function`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::WithChild::body` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::HorizontalStack::WithChild::stack` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::LiveTick` (`module`)

*No documentation provided.*

---

### `Console::Controls::LiveTick::LiveTickState` (`type`)

*No documentation provided.*

---

### `Console::Controls::LiveTick::LiveTickState::hasProgressBar` (`field`)

*No documentation provided.*

---

### `Console::Controls::LiveTick::LiveTickState::progressBar` (`field`)

*No documentation provided.*

---

### `Console::Controls::LiveTick::New` (`function`)

*No documentation provided.*

---

### `Console::Controls::LiveTick::Pulse` (`function`)

*No documentation provided.*

---

### `Console::Controls::LiveTick::Pulse::state` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::LiveTick::RegisterProgressBar` (`function`)

*No documentation provided.*

---

### `Console::Controls::LiveTick::RegisterProgressBar::bar` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::LiveTick::RegisterProgressBar::state` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel` (`module`)

*No documentation provided.*

---

### `Console::Controls::Panel::BodyLine` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::BodyLine::frame` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::BodyLine::innerWidth` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::BodyLine::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::ContentWidth` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::ContentWidth::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::Margin` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::Margin::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::Measure` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::Measure::available` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::Measure::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::New` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::New::body` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::Padding` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::Padding::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::Panel` (`type`)

*No documentation provided.*

---

### `Console::Controls::Panel::Panel::align` (`field`)

*No documentation provided.*

---

### `Console::Controls::Panel::Panel::body` (`field`)

*No documentation provided.*

---

### `Console::Controls::Panel::Panel::forceAscii` (`field`)

*No documentation provided.*

---

### `Console::Controls::Panel::Panel::margin` (`field`)

*No documentation provided.*

---

### `Console::Controls::Panel::Panel::padding` (`field`)

*No documentation provided.*

---

### `Console::Controls::Panel::Panel::title` (`field`)

*No documentation provided.*

---

### `Console::Controls::Panel::Render` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::Render::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::Render::size` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::RenderWithContext` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::RenderWithContext::ctx` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::RenderWithContext::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::RenderWithContext::size` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::TopBorder` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::TopBorder::frame` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::TopBorder::innerWidth` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::TopBorder::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::UseUnicodeFrame` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::UseUnicodeFrame::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::WithAlignment` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::WithAlignment::align` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::WithAlignment::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::WithAsciiFrame` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::WithAsciiFrame::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::WithPadding` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::WithPadding::padding` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::WithPadding::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::WithTitle` (`function`)

*No documentation provided.*

---

### `Console::Controls::Panel::WithTitle::panel` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::Panel::WithTitle::title` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar` (`module`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::BarBody` (`function`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::BarBody::bar` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::BarBody::width` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::Measure` (`function`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::Measure::available` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::Measure::bar` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::New` (`function`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::OnTick` (`function`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::OnTick::bar` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::ProgressBar` (`type`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::ProgressBar::anchorCol` (`field`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::ProgressBar::anchorRow` (`field`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::ProgressBar::onTick` (`field`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::ProgressBar::percent` (`field`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::Render` (`function`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::Render::bar` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::Render::size` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::RenderIncremental` (`function`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::RenderIncremental::bar` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::RenderIncremental::size` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::Tick` (`function`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::Tick::bar` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::WithAnchor` (`function`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::WithAnchor::bar` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::WithAnchor::col` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::WithAnchor::row` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::WithPercent` (`function`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::WithPercent::bar` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::ProgressBar::WithPercent::percent` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext` (`module`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::AdvanceRow` (`function`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::AdvanceRow::ctx` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::EraseLineAndRender` (`function`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::EraseLineAndRender::ctx` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::EraseLineAndRender::line` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::MoveTo` (`function`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::MoveTo::col` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::MoveTo::ctx` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::MoveTo::row` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::New` (`function`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::New::col` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::New::row` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::RenderAt` (`function`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::RenderAt::ctx` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::RenderAt::text` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::RenderContext` (`type`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::RenderContext::cursorCol` (`field`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::RenderContext::cursorRow` (`field`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::RenderContext::incremental` (`field`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::RenderContext::originCol` (`field`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::RenderContext::originRow` (`field`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::WithoutIncremental` (`function`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::WithoutIncremental::col` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::RenderContext::WithoutIncremental::row` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack` (`module`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::ChildCount` (`function`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::ChildCount::stack` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::Measure` (`function`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::Measure::available` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::Measure::stack` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::New` (`function`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::Render` (`function`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::Render::size` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::Render::stack` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::RenderWithContext` (`function`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::RenderWithContext::ctx` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::RenderWithContext::size` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::RenderWithContext::stack` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::VerticalStack` (`type`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::VerticalStack::childBodies` (`field`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::VerticalStack::childCount` (`field`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::WithChild` (`function`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::WithChild::body` (`parameter`)

*No documentation provided.*

---

### `Console::Controls::VerticalStack::WithChild::stack` (`parameter`)

*No documentation provided.*

---

### `Console::Format` (`module`)

*No documentation provided.*

---

### `Console::Format::Attributes::ApplyAttrList` (`function`)

*No documentation provided.*

---

### `Console::Format::Attributes::ApplyAttrList::attrs` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ApplyAttrList::chain` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ApplyAttrToken` (`function`)

*No documentation provided.*

---

### `Console::Format::Attributes::ApplyAttrToken::chain` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ApplyAttrToken::token` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseColor` (`function`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseColor::value` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseDecimalDigit` (`function`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseDecimalDigit::c` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseHexByte` (`function`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseHexByte::two` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseHexColor` (`function`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseHexColor::hex` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseHexNibble` (`function`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseHexNibble::digit` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseNamedColor` (`function`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseNamedColor::name` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseRgbTriplet` (`function`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseRgbTriplet::value` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseU8` (`function`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParseU8::digits` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParsedNibble` (`type`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParsedNibble::ok` (`field`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParsedNibble::value` (`field`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParsedRgb` (`type`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParsedRgb::b` (`field`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParsedRgb::g` (`field`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParsedRgb::ok` (`field`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParsedRgb::r` (`field`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParsedU8` (`type`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParsedU8::ok` (`field`)

*No documentation provided.*

---

### `Console::Format::Attributes::ParsedU8::value` (`field`)

*No documentation provided.*

---

### `Console::Format::Format` (`function`)

Renders markup to a styled string (plain text when ANSI disabled).

**Parameter `source`**
Markup input.


---

### `Console::Format::Format::source` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Markdown::IsEscapableSigil` (`function`)

*No documentation provided.*

---

### `Console::Format::Markdown::IsEscapableSigil::c` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Markdown::RenderInner` (`function`)

*No documentation provided.*

---

### `Console::Format::Markdown::RenderInner::ansi` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Markdown::RenderInner::s` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Markdown::RenderPlain` (`function`)

*No documentation provided.*

---

### `Console::Format::Markdown::RenderPlain::source` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Markdown::RenderStyled` (`function`)

*No documentation provided.*

---

### `Console::Format::Markdown::RenderStyled::source` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::ContainsSubstring` (`function`)

*No documentation provided.*

---

### `Console::Format::Scan::ContainsSubstring::haystack` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::ContainsSubstring::needle` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::Drop` (`function`)

*No documentation provided.*

---

### `Console::Format::Scan::Drop::count` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::Drop::text` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::IndexOfFrom` (`function`)

*No documentation provided.*

---

### `Console::Format::Scan::IndexOfFrom::haystack` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::IndexOfFrom::needle` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::IndexOfFrom::start` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::Len` (`function`)

*No documentation provided.*

---

### `Console::Format::Scan::Len::text` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::Slice` (`function`)

*No documentation provided.*

---

### `Console::Format::Scan::Slice::count` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::Slice::start` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::Slice::text` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::StartsWith` (`function`)

*No documentation provided.*

---

### `Console::Format::Scan::StartsWith::prefix` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::StartsWith::text` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::Trim` (`function`)

*No documentation provided.*

---

### `Console::Format::Scan::Trim::text` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::TrimLeft` (`function`)

*No documentation provided.*

---

### `Console::Format::Scan::TrimLeft::text` (`parameter`)

*No documentation provided.*

---

### `Console::Format::Scan::TrimRight` (`function`)

*No documentation provided.*

---

### `Console::Format::Scan::TrimRight::text` (`parameter`)

*No documentation provided.*

---

### `Console::Format::StripMarkup` (`function`)

Strips markup delimiters and emits visible text only.

**Parameter `source`**
Markup input.


---

### `Console::Format::StripMarkup::source` (`parameter`)

*No documentation provided.*

---

### `Console::FormatLine` (`function`)

Formats markup and writes to stdout with a trailing newline.

---

### `Console::FormatLine::text` (`parameter`)

*No documentation provided.*

---

### `Console::FormatWrite` (`function`)

Writes formatted markup to stdout without a newline.

---

### `Console::FormatWrite::text` (`parameter`)

*No documentation provided.*

---

### `Console::MessagesChannel` (`function`)

Default unbounded channel for resize and tick messages (cross-fiber UI loop).

---

### `Console::OnResize` (`type`)

Resize multicast surface; pair with `SubscribeOnResize` and `RunTick`.
This same-fiber event hub is separate from `Concurrency.Hub`.

---

### `Console::OnResize::OnResize` (`field`)

*No documentation provided.*

---

### `Console::OnResize::lastSize` (`field`)

*No documentation provided.*

---

### `Console::QuerySize` (`function`)

Returns the current terminal size (best-effort per host).

---

### `Console::RunTick` (`function`)

Polls terminal size and **Send**s `ConsoleMessage::Resize` when dimensions change.

---

### `Console::RunTick::lastSize` (`parameter`)

*No documentation provided.*

---

### `Console::RunTick::messages` (`parameter`)

*No documentation provided.*

---

### `Console::RunTickHub` (`function`)

In-fiber resize multicast (same-fiber handlers only); prefer `MessagesChannel` across fibers.

---

### `Console::RunTickHub::hub` (`parameter`)

*No documentation provided.*

---

### `Console::RunTickLive` (`function`)

Polls resize, publishes tick to `messages`, and advances registered live controls.

---

### `Console::RunTickLive::lastSize` (`parameter`)

*No documentation provided.*

---

### `Console::RunTickLive::live` (`parameter`)

*No documentation provided.*

---

### `Console::RunTickLive::messages` (`parameter`)

*No documentation provided.*

---

### `Console::ShouldStyle` (`function`)

Returns whether ANSI styling should be emitted for the current host.

---

### `Console::Start` (`function`)

Initializes platform terminal probes and seeds resize tracking on the hub.

---

### `Console::Start::hub` (`parameter`)

*No documentation provided.*

---

### `Console::Style` (`module`)

*No documentation provided.*

---

### `Console::Style::ClampWidth` (`function`)

*No documentation provided.*

---

### `Console::Style::ClampWidth::maxValue` (`parameter`)

*No documentation provided.*

---

### `Console::Style::ClampWidth::minValue` (`parameter`)

*No documentation provided.*

---

### `Console::Style::ClampWidth::value` (`parameter`)

*No documentation provided.*

---

### `Console::Style::Margin` (`enum`)

*No documentation provided.*

---

### `Console::Style::Margin::None` (`enum_variant`)

*No documentation provided.*

---

### `Console::Style::Margin::Sized` (`enum_variant`)

*No documentation provided.*

---

### `Console::Style::Margin::Value` (`enum_variant`)

*No documentation provided.*

---

### `Console::Style::Margin::size` (`field`)

*No documentation provided.*

---

### `Console::Style::Margin::value` (`field`)

*No documentation provided.*

---

### `Console::Style::Padding` (`enum`)

*No documentation provided.*

---

### `Console::Style::Padding::None` (`enum_variant`)

*No documentation provided.*

---

### `Console::Style::Padding::Sized` (`enum_variant`)

*No documentation provided.*

---

### `Console::Style::Padding::Value` (`enum_variant`)

*No documentation provided.*

---

### `Console::Style::Padding::size` (`field`)

*No documentation provided.*

---

### `Console::Style::Padding::value` (`field`)

*No documentation provided.*

---

### `Console::Style::ResolveMargin` (`function`)

*No documentation provided.*

---

### `Console::Style::ResolveMargin::columns` (`parameter`)

*No documentation provided.*

---

### `Console::Style::ResolveMargin::margin` (`parameter`)

*No documentation provided.*

---

### `Console::Style::ResolvePadding` (`function`)

*No documentation provided.*

---

### `Console::Style::ResolvePadding::columns` (`parameter`)

*No documentation provided.*

---

### `Console::Style::ResolvePadding::padding` (`parameter`)

*No documentation provided.*

---

### `Console::Style::ResolveSize` (`function`)

Resolves `Size` to a character count for the given terminal width.

**Parameter `size`**
Logical size token.


**Parameter `columns`**
Terminal width in columns.


---

### `Console::Style::ResolveSize::columns` (`parameter`)

*No documentation provided.*

---

### `Console::Style::ResolveSize::size` (`parameter`)

*No documentation provided.*

---

### `Console::Style::Size` (`enum`)

Shared layout/style enums for console controls.

---

### `Console::Style::Size::Large` (`enum_variant`)

*No documentation provided.*

---

### `Console::Style::Size::Medium` (`enum_variant`)

*No documentation provided.*

---

### `Console::Style::Size::Small` (`enum_variant`)

*No documentation provided.*

---

### `Console::Style::TitleAlignment` (`enum`)

*No documentation provided.*

---

### `Console::Style::TitleAlignment::Center` (`enum_variant`)

*No documentation provided.*

---

### `Console::Style::TitleAlignment::Left` (`enum_variant`)

*No documentation provided.*

---

### `Console::Style::TitleAlignment::Right` (`enum_variant`)

*No documentation provided.*

---

### `Console::SubscribeOnResize` (`function`)

Subscribes to resize notifications and synchronously raises once with the current size.

---

### `Console::SubscribeOnResize::handler` (`parameter`)

*No documentation provided.*

---

### `Console::SubscribeOnResize::hub` (`parameter`)

*No documentation provided.*

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

### `Platform::Linux::IsAtty` (`function`)

*No documentation provided.*

---

### `Platform::Linux::IsAtty::fd` (`parameter`)

*No documentation provided.*

---

### `Platform::Linux::Libc` (`contract`)

Linux terminal probes via libc Extern (`isatty`, best-effort `ioctl` winsize).

---

### `Platform::Linux::Libc::arg` (`parameter`)

*No documentation provided.*

---

### `Platform::Linux::Libc::fd` (`parameter`)

*No documentation provided.*

---

### `Platform::Linux::Libc::fd` (`parameter`)

*No documentation provided.*

---

### `Platform::Linux::Libc::ioctl` (`contract_method`)

`TIOCGWINSZ` (0x5413); `arg` is `struct winsize*` when host linking is enabled.

---

### `Platform::Linux::Libc::isatty` (`contract_method`)

*No documentation provided.*

---

### `Platform::Linux::Libc::request` (`parameter`)

*No documentation provided.*

---

### `Platform::Linux::WinSize` (`type`)

Packed `struct winsize` layout for ioctl out-parameters (host ABI).

---

### `Platform::Linux::WinSize::ws_col` (`field`)

*No documentation provided.*

---

### `Platform::Linux::WinSize::ws_row` (`field`)

*No documentation provided.*

---

### `Platform::Linux::WinSize::ws_xpixel` (`field`)

*No documentation provided.*

---

### `Platform::Linux::WinSize::ws_ypixel` (`field`)

*No documentation provided.*

---

### `Platform::Linux::Winsize` (`function`)

Returns terminal size from ioctl when available; otherwise zeroed columns for env fallback.
`TIOCGWINSZ` on Linux; ioctl needs a live pointer at link/runtime (see platform spec).

---

### `Platform::MacOS::IsAtty` (`function`)

*No documentation provided.*

---

### `Platform::MacOS::IsAtty::fd` (`parameter`)

*No documentation provided.*

---

### `Platform::MacOS::Libc` (`contract`)

macOS terminal probes via libc Extern (`isatty`, best-effort `ioctl` winsize).

---

### `Platform::MacOS::Libc::arg` (`parameter`)

*No documentation provided.*

---

### `Platform::MacOS::Libc::fd` (`parameter`)

*No documentation provided.*

---

### `Platform::MacOS::Libc::fd` (`parameter`)

*No documentation provided.*

---

### `Platform::MacOS::Libc::ioctl` (`contract_method`)

`TIOCGWINSZ` (0x40087468 on macOS); `arg` is `struct winsize*` when host linking is enabled.

---

### `Platform::MacOS::Libc::isatty` (`contract_method`)

*No documentation provided.*

---

### `Platform::MacOS::Libc::request` (`parameter`)

*No documentation provided.*

---

### `Platform::MacOS::WinSize` (`type`)

*No documentation provided.*

---

### `Platform::MacOS::WinSize::ws_col` (`field`)

*No documentation provided.*

---

### `Platform::MacOS::WinSize::ws_row` (`field`)

*No documentation provided.*

---

### `Platform::MacOS::WinSize::ws_xpixel` (`field`)

*No documentation provided.*

---

### `Platform::MacOS::WinSize::ws_ypixel` (`field`)

*No documentation provided.*

---

### `Platform::MacOS::Winsize` (`function`)

*No documentation provided.*

---

### `Platform::Terminal` (`module`)

*No documentation provided.*

---

### `Platform::Terminal::EnsureInitialized` (`function`)

*No documentation provided.*

---

### `Platform::Terminal::EnvEquals` (`function`)

*No documentation provided.*

---

### `Platform::Terminal::EnvEquals::expected` (`parameter`)

*No documentation provided.*

---

### `Platform::Terminal::EnvEquals::name` (`parameter`)

*No documentation provided.*

---

### `Platform::Terminal::EnvFallbackSize` (`function`)

Best-effort parse of `COLUMNS` / `LINES` when numeric ioctl paths are unavailable.

---

### `Platform::Terminal::EnvFlagSet` (`function`)

*No documentation provided.*

---

### `Platform::Terminal::EnvFlagSet::name` (`parameter`)

*No documentation provided.*

---

### `Platform::Terminal::ForcePlainText` (`function`)

*No documentation provided.*

---

### `Platform::Terminal::IsAtty` (`function`)

*No documentation provided.*

---

### `Platform::Terminal::IsAtty::fd` (`parameter`)

*No documentation provided.*

---

### `Platform::Terminal::ParseEnvColumns` (`function`)

*No documentation provided.*

---

### `Platform::Terminal::ParseEnvColumns::defaultValue` (`parameter`)

*No documentation provided.*

---

### `Platform::Terminal::ParseEnvColumns::value` (`parameter`)

*No documentation provided.*

---

### `Platform::Terminal::ParseEnvRows` (`function`)

*No documentation provided.*

---

### `Platform::Terminal::ParseEnvRows::defaultValue` (`parameter`)

*No documentation provided.*

---

### `Platform::Terminal::ParseEnvRows::value` (`parameter`)

*No documentation provided.*

---

### `Platform::Terminal::PollResize` (`function`)

Publishes `ConsoleMessage::Resize` when terminal dimensions change.

---

### `Platform::Terminal::PollResize::lastSize` (`parameter`)

*No documentation provided.*

---

### `Platform::Terminal::PollResize::messages` (`parameter`)

*No documentation provided.*

---

### `Platform::Terminal::PollResizeHub` (`function`)

In-fiber resize hub (event multicast on the owning fiber).

---

### `Platform::Terminal::PollResizeHub::hub` (`parameter`)

*No documentation provided.*

---

### `Platform::Terminal::ProbeColorModel` (`function`)

*No documentation provided.*

---

### `Platform::Terminal::QuerySize` (`function`)

*No documentation provided.*

---

### `Platform::Windows::IsAtty` (`function`)

*No documentation provided.*

---

### `Platform::Windows::IsAtty::fd` (`parameter`)

*No documentation provided.*

---

### `Platform::Windows::Kernel32` (`contract`)

*No documentation provided.*

---

### `Platform::Windows::Kernel32::GetConsoleScreenBufferInfo` (`contract_method`)

*No documentation provided.*

---

### `Platform::Windows::Kernel32::GetStdHandle` (`contract_method`)

*No documentation provided.*

---

### `Platform::Windows::Kernel32::handle` (`parameter`)

*No documentation provided.*

---

### `Platform::Windows::Kernel32::infoOut` (`parameter`)

*No documentation provided.*

---

### `Platform::Windows::Kernel32::nStdHandle` (`parameter`)

*No documentation provided.*

---

### `Platform::Windows::Ucrt` (`contract`)

Windows console probes (`_isatty`, best-effort `GetConsoleScreenBufferInfo`).

---

### `Platform::Windows::Ucrt::_isatty` (`contract_method`)

*No documentation provided.*

---

### `Platform::Windows::Ucrt::fd` (`parameter`)

*No documentation provided.*

---

### `Platform::Windows::Winsize` (`function`)

Returns size from console buffer info when host linking supplies struct out-pointers; else zeroed.

---

### `Query::Contracts` (`module`)

Re-exports query contracts (Option type).

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

### `System::Threading::Thread` (`module`)

*No documentation provided.*

---

### `System::Threading::Thread::DarwinPthread` (`contract`)

macOS libc pthread surface (v1).

---

### `System::Threading::Thread::DarwinPthread::arg` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::DarwinPthread::attr` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::DarwinPthread::pthread_create` (`contract_method`)

*No documentation provided.*

---

### `System::Threading::Thread::DarwinPthread::pthread_join` (`contract_method`)

*No documentation provided.*

---

### `System::Threading::Thread::DarwinPthread::sched_yield` (`contract_method`)

*No documentation provided.*

---

### `System::Threading::Thread::DarwinPthread::startRoutine` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::DarwinPthread::thread` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::DarwinPthread::threadOut` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::DarwinPthread::valueOut` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::Join` (`function`)

Waits for OS thread completion.

---

### `System::Threading::Thread::Join::self` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::LinuxPthread` (`contract`)

Linux libc pthread surface (v1).

---

### `System::Threading::Thread::LinuxPthread::arg` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::LinuxPthread::attr` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::LinuxPthread::pthread_create` (`contract_method`)

*No documentation provided.*

---

### `System::Threading::Thread::LinuxPthread::pthread_join` (`contract_method`)

*No documentation provided.*

---

### `System::Threading::Thread::LinuxPthread::sched_yield` (`contract_method`)

*No documentation provided.*

---

### `System::Threading::Thread::LinuxPthread::startRoutine` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::LinuxPthread::thread` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::LinuxPthread::threadOut` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::LinuxPthread::valueOut` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::Spawn` (`function`)

Starts an OS thread. Beskid entry attachment is enforced by the runtime before user code runs.

**Returns**

`Thread` handle or `ThreadError::SpawnFailed` when pthread creation fails.


---

### `System::Threading::Thread::Spawn::arg` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::Spawn::entryRoutine` (`parameter`)

*No documentation provided.*

---

### `System::Threading::Thread::Thread` (`type`)

Opaque pthread handle on Linux and macOS.

---

### `System::Threading::Thread::Thread::handle` (`field`)

*No documentation provided.*

---

### `System::Threading::Thread::Yield` (`function`)

OS-level yield (not `Concurrency.Yield`).

---

### `System::Threading::ThreadError` (`module`)

*No documentation provided.*

---

### `System::Threading::ThreadError::ThreadError` (`enum`)

Errors from `System.Threading.Thread.Spawn`.

**Variant `SpawnFailed`**
Host rejected thread creation.


---

### `System::Threading::ThreadError::ThreadError::SpawnFailed` (`enum_variant`)

*No documentation provided.*

---

### `System::Threading::ThreadError::ThreadError::code` (`field`)

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

### `range` (`function`)

*No documentation provided.*

---


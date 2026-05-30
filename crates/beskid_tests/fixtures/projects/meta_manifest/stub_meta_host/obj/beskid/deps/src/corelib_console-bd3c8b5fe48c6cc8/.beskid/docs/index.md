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
- `Concurrency`
  - `Channel`
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
    - `Concurrency::ChannelOptions` (`module`)
  - `Fiber`
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
    - `Concurrency::HubReceiveResult` (`module`)
  - `Mutex`
    - `Concurrency::Mutex` (`module`)
  - `MutexError`
    - `MutexError`
      - `Cancelled`
        - `Concurrency::MutexError::MutexError::Cancelled` (`enum_variant`)
      - `Concurrency::MutexError::MutexError` (`enum`)
    - `Concurrency::MutexError` (`module`)
  - `MutexGuard`
    - `Concurrency::MutexGuard` (`module`)
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
    - `Concurrency::WaitGroup` (`module`)
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
- `Platform`
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
  - `Threading`
    - `Thread`
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

### `Concurrency` (`module`)

Concurrency package prelude: cooperative fibers, channels, and OS threading.

---

### `Concurrency::Channel` (`module`)

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

### `Concurrency::Fiber` (`module`)

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

### `Concurrency::Mutex` (`module`)

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


<a id="v0.7.0"></a>
# [v0.7.0](https://github.com/rhysd/tui-textarea/releases/tag/v0.7.0) - 2024-10-22

- **BREAKING:** Update [ratatui](https://ratatui.rs/) crate dependency from v0.28 to [v0.29](https://github.com/ratatui-org/ratatui/releases/tag/v0.29.0). ([#86](https://github.com/rhysd/tui-textarea/issues/86), thanks [@ricott1](https://github.com/ricott1))

[Changes][v0.7.0]


<a id="v0.6.1"></a>
# [v0.6.1](https://github.com/rhysd/tui-textarea/releases/tag/v0.6.1) - 2024-08-08

- Add [`TextArea::selection_range`](https://docs.rs/tui-textarea/latest/tui_textarea/struct.TextArea.html#method.selection_range) method to get the range of the current selection. Please read the document for more details. ([#81](https://github.com/rhysd/tui-textarea/issues/81), thanks [@achristmascarl](https://github.com/achristmascarl))
  ```rust
  let mut textarea = TextArea::from(["aaa"]);

  // It returns `None` when the text selection is not ongoing
  assert_eq!(textarea.selection_range(), None);

  textarea.start_selection();
  assert_eq!(textarea.selection_range(), Some(((0, 0), (0, 0))));

  textarea.move_cursor(CursorMove::Forward);
  assert_eq!(textarea.selection_range(), Some(((0, 0), (0, 1))));

  // The first element of the pair is always smaller than the second one.
  textarea.start_selection();
  textarea.move_cursor(CursorMove::Back);
  assert_eq!(textarea.selection_range(), Some(((0, 0), (0, 1))));
  ```
- Fix depending on the incorrect version of termion crate when `tuirs-termion` feature is enabled. Since tui crate depends on older version of termion crate v1.5.6, tui-textarea should depend on the same version but actually it depended on the latest version v4.0.0.
  - If you use tui-textarea with tui crate and termion crate, please ensure that your project also depends on termion v1.5. Otherwise your project accidentally depends on multiple versions of termion crate.

[Changes][v0.6.1]


<a id="v0.6.0"></a>
# [v0.6.0](https://github.com/rhysd/tui-textarea/releases/tag/v0.6.0) - 2024-08-07

- **BREAKING:** Update [ratatui](https://ratatui.rs/) crate dependency from v0.27 to [v0.28](https://github.com/ratatui-org/ratatui/releases/tag/v0.28.0).
- **BREAKING:** Update [crossterm](https://crates.io/crates/crossterm) crate dependency from v0.27 to v0.28 because ratatui crate depends on the new version.
  - Note: If you use tui crate, crossterm crate dependency remains at v0.25.

[Changes][v0.6.0]


<a id="v0.5.3"></a>
# [v0.5.3](https://github.com/rhysd/tui-textarea/releases/tag/v0.5.3) - 2024-08-03

- `&TextArea` now implements `Widget` trait. ([#78](https://github.com/rhysd/tui-textarea/issues/78))
  - Now the reference can be passed to `ratatui::terminal::Frame::render_widget` method call directly.
    ```rust
    // v0.5.2 or earlier
    f.render_widget(textarea.widget(), rect);

    // v0.5.3 or later
    f.render_widget(&textarea, rect);
    ```
  - This means that `TextArea::widget` method is no longer necessary. To maintain the compatibility the method is not removed but using it starts to report a deprecation warning from v0.5.3.
- Fix a cursor can leave the viewport on horizontal scroll when line number is displayed. ([#77](https://github.com/rhysd/tui-textarea/issues/77))
- Support some key combinations added at termion v4 for `termion` feature. ([#68](https://github.com/rhysd/tui-textarea/issues/68))
  - `termion::event::Key::CtrlLeft`
  - `termion::event::Key::CtrlRight`
  - `termion::event::Key::CtrlUp`
  - `termion::event::Key::CtrlDown`
  - `termion::event::Key::CtrlHome`
  - `termion::event::Key::CtrlEnd`
  - `termion::event::Key::AltLeft`
  - `termion::event::Key::AltRight`
  - `termion::event::Key::AltUp`
  - `termion::event::Key::AltDown`
  - `termion::event::Key::ShiftLeft`
  - `termion::event::Key::ShiftRight`
  - `termion::event::Key::ShiftUp`
  - `termion::event::Key::ShiftDown`
- Fix the border color is not applied in `single_line` example. ([#79](https://github.com/rhysd/tui-textarea/issues/79), thanks [@fmorroni](https://github.com/fmorroni))
- Improve `vim` example's Vim emulation.
  - Fix the range of text selection on `e` mapping in operator-pending mode. ([#76](https://github.com/rhysd/tui-textarea/issues/76))
  - Fix the text selection on `y`, `d`, `c` mappings in visual mode is not inclusive.

[Changes][v0.5.3]


<a id="v0.5.2"></a>
# [v0.5.2](https://github.com/rhysd/tui-textarea/releases/tag/v0.5.2) - 2024-08-01

- Do not hide a cursor when a placeholder text is printed. ([#73](https://github.com/rhysd/tui-textarea/issues/73), thanks [@kyu08](https://github.com/kyu08))
  - ![demo](https://raw.githubusercontent.com/rhysd/ss/master/tui-textarea/placepop.gif)
- Add [`CursorMove::WordEnd`](https://docs.rs/tui-textarea/0.5.2/tui_textarea/enum.CursorMove.html#variant.WordEnd) which moves a cursor to the end of the next word inclusively. ([#75](https://github.com/rhysd/tui-textarea/issues/75), thanks [@achristmascarl](https://github.com/achristmascarl))
  - The behavior is similar to `e` mapping of Vim in normal mode. [`vim` example](https://github.com/rhysd/tui-textarea/blob/main/examples/vim.rs) implements the mapping for demonstration.

[Changes][v0.5.2]


<a id="v0.5.1"></a>
# [v0.5.1](https://github.com/rhysd/tui-textarea/releases/tag/v0.5.1) - 2024-07-12

- Add `serde` optional feature. When it is enabled, some types support the serialization/deserialization with [serde](https://crates.io/crates/serde) crate. See [the document](https://github.com/rhysd/tui-textarea?tab=readme-ov-file#serializationdeserialization-support) for more details. ([#62](https://github.com/rhysd/tui-textarea/issues/62), thanks [@cestef](https://github.com/cestef))
  ```rust
  use tui_textarea::Input;

  let json = r#"
      {
          "key": { "Char": "a" },
          "ctrl": true,
          "alt": false,
          "shift": true
      }
  "#;

  let input: Input = serde_json::from_str(json).unwrap();
  println!("{input}");
  // Input {
  //     key: Key::Char('a'),
  //     ctrl: true,
  //     alt: false,
  //     shift: true,
  // }
  ```


[Changes][v0.5.1]


<a id="v0.5.0"></a>
# [v0.5.0](https://github.com/rhysd/tui-textarea/releases/tag/v0.5.0) - 2024-07-07

This is a maintenance release for supporting recent versions of [ratatui](https://crates.io/crates/ratatui) crate.

- **BREAKING CHANGE:** Bump the minimal versions of the following dependencies. If you're depending on the crates older than the following versions, please upgrade them before upgrading this crate. ([#69](https://github.com/rhysd/tui-textarea/issues/69), thanks [@joshka](https://github.com/joshka))
  -  ratatui 0.27.0
  - termion 0.4.0
  - termwiz 0.22.0
- `YankText` now implements `Display` instead of `ToString` directly. Since `ToString` is implemented for any types which implement `Display`, this is not a breaking change.

[Changes][v0.5.0]


<a id="v0.4.0"></a>
# [v0.4.0](https://github.com/rhysd/tui-textarea/releases/tag/v0.4.0) - 2023-11-19

This release introduces text selection feature. The internal implementation was largely refactored to handle multi-line text for this feature. As the side effect, several APIs now can handle a multi-line string (string contains newlines) correctly.

- Text selection has been implemented. ([#6](https://github.com/rhysd/tui-textarea/issues/6), [#45](https://github.com/rhysd/tui-textarea/issues/45), thanks [@pm100](https://github.com/pm100) for the first implementation)
  <img src="https://github.com/rhysd/tui-textarea/assets/823277/2ad4e4ba-3628-44c2-b61c-f0b270d09e27" width=590 height=156 alt="minimal example">
  - Default key shortcuts now support text selection. When moving the cursor with pressing a shift key, a textarea starts to select the text under the cursor. The selected text can be copied/cut by the following key shortcuts. Modifying some text while text selection deletes the selected text. Doing undo/redo cancels the ongoing text selection.
    | Mappings         | Description        |
    |------------------|--------------------|
    | `Ctrl+C`, `Copy` | Copy selected text |
    | `Ctrl+X`, `Cut`  | Cut selected text  |
  - The following APIs are added
    - `TextArea::copy` keeps the selected text as a yanked text
    - `TextArea::cut` deletes the selected text and keeps it as a yanked text
    - `TextArea::start_selection` starts text selection
    - `TextArea::cancel_selection` cancels text selection
    - `TextArea::select_all` selects the entire text
    - `TextArea::set_selection_style` sets the style of selected text
    - `TextArea::selection_style` returns the current style for selected text
- **BREAKING CHANGE:** `col` argument of `TextArea::delete_str` was removed. Instead, current cursor position is used. This change is for aligninig the API signature with `TextArea::insert_str`.
  - Before: `fn delete_str(&mut self, col: usize, chars: usize) -> bool`
  - After: `fn delete_str(&mut self, chars: usize) -> bool`
- **BREAKING CHANGE:** `TextArea::yank_text` now returns `String` instead of `&str`. This change was caused to handle yanking multiple-line text correctly.
  - Before: `fn yank_text<'a>(&'a self) -> &'a str`
  - After: `fn yank_text(&self) -> String`
- **BREAKING CHANGE:** `shift` field was added to `Input` to support the Shift modifier key.
- Add `Key::Paste`, `Key::Copy`, and `Key::Cut`. They are only supported by termwiz crate.
- Fix `TextArea::insert_char` didn't handle newline (`'\n'`) correctly.
- Allow passing multi-line string to `TextArea::insert_str`. A string joined with newlines is inserted as multiple lines correctly.
- Allow `TextArea::delete_str` to delete multiple lines ([#42](https://github.com/rhysd/tui-textarea/issues/42)).
- Fix `TextArea::set_yank_text` didn't handle multiple lines correctly.
- Fix `editor` example didn't handle terminal raw mode on Windows ([#44](https://github.com/rhysd/tui-textarea/issues/44)).
- `modal` example was rebuilt as [`vim` example](https://github.com/rhysd/tui-textarea/blob/main/examples/vim.rs). It implements Vim emulation to some level as a state machine. It adds the support for very basic visual mode and operator-pending mode. This example aims to show how to implement complicated and stateful key shortcuts.
- Add many unit test cases. Several edge cases found by them were fixed. The [code coverage](https://app.codecov.io/gh/rhysd/tui-textarea) of this crate reached 90%.

[Changes][v0.4.0]


<a id="v0.3.1"></a>
# [v0.3.1](https://github.com/rhysd/tui-textarea/releases/tag/v0.3.1) - 2023-11-04

- Fix the width of rendered tab character (`\t`) is wrong in some cases when hard tab is enabled by `TextArea::set_hard_tab_indent` ([#43](https://github.com/rhysd/tui-textarea/issues/43)).
- Fix key inputs are doubled on Windows when converting from `crossterm::event::KeyEvent` into `tui_textarea::Input`. Note that the conversion from `crossterm::event::Event` into `tui_textarea::Input` does not have this issue.
- Support converting the following type instances into `tui_textarea::Input`.
  - `crossterm::event::KeyCode`
  - `crossterm::event::KeyEvent`
  - `crossterm::event::MouseEvent`
  - `crossterm::event::MouseKind`
  - `termwiz::input::KeyCode`
  - `termwiz::input::KeyEvent`
  - `termion::event::MouseButton`
- Fix typos in API document and error message ([#40](https://github.com/rhysd/tui-textarea/issues/40), thanks [@fritzrehde](https://github.com/fritzrehde)).

[Changes][v0.3.1]


<a id="v0.3.0"></a>
# [v0.3.0](https://github.com/rhysd/tui-textarea/releases/tag/v0.3.0) - 2023-10-24

- **BREAKING CHANGE:** Enable ratatui support by default instead of inactive tui-rs.
  - `ratatui-` prefix is removed from all `ratatui-*` features. `crossterm`, `termion`, and `termwiz` features are for ratatui:
    ```yaml
    # ratatui with crossterm backend
    tui-textarea = "0.3"
    # ratatui with termwiz backend
    tui-textarea = { version = "0.3", features = ["termwiz"], default-features = false }
    # ratatui with termion backend
    tui-textarea = { version = "0.3", features = ["termion"], default-features = false }
    ```
  - Instead, features for tui-rs support are now prefixed with `tuirs-`:
    ```yaml
    # tui-rs with crossterm backend
    tui-textarea = { version = "0.3", features = ["tuirs-crossterm"], default-features = false }
    # Use proper version of crossterm
    crossterm = "0.2.5"
    ```
  - Examples and documents are now implemented and described with ratatui by default
- **BREAKING CHANGE:** Rename `your-backend` features to `no-backend`. You need to update the feature names if you're using tui-textarea with your own backend.
- Relax the restriction of ratatui crate dependency from `0.23.0` to `>=0.23.0, <1`, which means 'v0.23.0 or later and earlier than v1'. The latest version of ratatui (v0.24.0) now works with tui-textarea ([#36](https://github.com/rhysd/tui-textarea/issues/36)).
- Enable `termwiz` and `termion` features on generating the API document. APIs to convert from input events of termwiz/termion to [`tui_textarea::Input`](https://docs.rs/tui-textarea/latest/tui_textarea/struct.Input.html) are now listed in the document.

Previous Backend features table (v0.2.4):

|         | crossterm                        | termion           | termwiz           | Your own backend       |
|---------|----------------------------------|-------------------|-------------------|------------------------|
| tui-rs  | `crossterm` (enabled by default) | `termion`         | N/A               | `your-backend`         |
| ratatui | `ratatui-crossterm`              | `ratatui-termion` | `ratatui-termwiz` | `ratatui-your-backend` |

New backend features table (v0.3.0):

|         | crossterm                        | termion         | termwiz   | Your own backend   |
|---------|----------------------------------|-----------------|-----------|--------------------|
| tui-rs  | `tuirs-crossterm`                | `tuirs-termion` | N/A       | `tuirs-no-backend` |
| ratatui | `crossterm` (enabled by default) | `termion`       | `termwiz` | `no-backend`       |


[Changes][v0.3.0]


<a id="v0.2.4"></a>
# [v0.2.4](https://github.com/rhysd/tui-textarea/releases/tag/v0.2.4) - 2023-10-21

- Support the ratatui's [termwiz](https://crates.io/crates/termwiz) backend. `ratatui-termwiz` feature was newly added for this.
  - Add the following dependencies in your Cargo.toml to use termwiz support.
    ```toml
    termwiz = "0.20"
    ratatui = { version = "0.23", default-features = false, features = ["termwiz"] }
    tui-textarea = { version = "0.2.4", default-features = false, features = ["ratatui-termwiz"] }
    ```
  - Read and run [the `termwiz` example](https://github.com/rhysd/tui-textarea/blob/main/examples/termwiz.rs) to know the API usage.
    ```sh
    cargo run --example termwiz --no-default-features --features=ratatui-termwiz
    ```
- Fix calculating the length of tab character when the line contains wide characters. Now the length of wide characters like あ are calculated as 2 correctly.

[Changes][v0.2.4]


<a id="v0.2.3"></a>
# [v0.2.3](https://github.com/rhysd/tui-textarea/releases/tag/v0.2.3) - 2023-10-20

- Add APIs to mask text with a character ([#32](https://github.com/rhysd/tui-textarea/issues/32), thanks [@pm100](https://github.com/pm100)).
  - `TextArea::set_mask_char`, `TextArea::clear_mask_char`, `TextArea::mask_char` are added. See [the documentation](https://docs.rs/tui-textarea/latest/tui_textarea/) for more details.
  - The [`password`](https://github.com/rhysd/tui-textarea/blob/main/examples/password.rs) example was added to show the usage.
    <img src="https://raw.githubusercontent.com/rhysd/ss/master/tui-textarea/password.gif" width=589 height=92 alt="password example">
- Fix the length of displayed hard tab in text ([#33](https://github.com/rhysd/tui-textarea/issues/33), thanks [@pm100](https://github.com/pm100)).

[Changes][v0.2.3]


<a id="v0.2.2"></a>
# [v0.2.2](https://github.com/rhysd/tui-textarea/releases/tag/v0.2.2) - 2023-10-01

Very small patch release only for fixing [the build failure on docs.rs](https://docs.rs/crate/tui-textarea/0.2.1/builds/926847). No implementation has been changed.

[Changes][v0.2.2]


<a id="v0.2.1"></a>
# [v0.2.1](https://github.com/rhysd/tui-textarea/releases/tag/v0.2.1) - 2023-10-01

- Add the support for [ratatui](https://crates.io/crates/ratatui) crate in addition to [tui-rs](https://crates.io/crates/tui). The ratatui crate is a community fork of inactive tui-rs crate. ([#12](https://github.com/rhysd/tui-textarea/issues/12))
  - The latest version of ratatui v0.23 is supported.
  - tui-textarea still uses tui-rs by default to keep the compatibility at this moment. ratatui users explicitly need to set features for it. See [the installation document](https://github.com/rhysd/tui-textarea#installation) for the features matrix. For example, when you want to use ratatui and crossterm, write the following in your `Cargo.toml`:
    ```toml
    [dependencies]
    ratatui = "*"
    tui-textarea = { version = "*", features = ["ratatui-crossterm"], default-features = false }
    ```
  - tui-rs is no longer maintained and the repository was archived. At the next minor version bump, tui-textarea will switch the default features from tui-rs to ratatui. If you use tui-rs, I recommend to switch your dependency to ratatui.
  - Examples with ratatui are added to [the `examples` directory](https://github.com/rhysd/tui-textarea/tree/main/examples). For example, the following command runs ratatui version of `editor` example:
    ```sh
    cargo run --example ratatui_editor --no-default-features --features=ratatui-crossterm,search file.txt
    ```
- Add support for the placeholder text which is rendered when no text is input in the textarea. ([#16](https://github.com/rhysd/tui-textarea/issues/16), thanks [@pm100](https://github.com/pm100))
  - Use `TextArea::set_placeholder_text` to set the text. To change the text style, use `TextArea::set_placeholder_style`. See [the API documentation](https://docs.rs/tui-textarea/latest/tui_textarea/struct.TextArea.html) for more details.
  - `popup_placeholder` example was added to show the usage.
    ```sh
    cargo run --example popup_placeholder
    ```
- Derive `Debug` trait for `TextArea` struct. ([#23](https://github.com/rhysd/tui-textarea/issues/23))
- Fix a key input is received twice on Windows. ([#17](https://github.com/rhysd/tui-textarea/issues/17), thanks [@pm100](https://github.com/pm100))

[Changes][v0.2.1]


<a id="v0.2.0"></a>
# [v0.2.0](https://github.com/rhysd/tui-textarea/releases/tag/v0.2.0) - 2022-10-18

- Add [`Scrolling` enum](https://docs.rs/tui-textarea/latest/tui_textarea/enum.Scrolling.html) to provide more flexible scrolling via [`TextArea::scroll`](https://docs.rs/tui-textarea/latest/tui_textarea/struct.TextArea.html#method.scroll) method. It has the following enum variants.
  - **BREAKING** `Scrolling::Delta` scrolls the textarea by given rows and cols. This variant can be converted from `(i16, i16)` so migrating from v0.1.6 is very easy.
    ```rust
    let rows: i16 = ...;
    let cols: i16 = ...;

    // Until v0.1.6
    textarea.scroll(rows, cols);

    // Since v0.2.0
    textarea.scroll((rows, cols));
    ```
  - `Scrolling::PageDown` and `Scrolling::PageUp` scroll the textarea by page.
  - `Scrolling::HalfPageDown` and `Scrolling::HalfPageUp` scroll the textarea by half-page.
- Update default key mappings handled by [`TextArea::input`](https://docs.rs/tui-textarea/latest/tui_textarea/struct.TextArea.html#method.input) method.
  - **BREAKING** Change `PageDown` and `PageUp` keys to scroll down/up the textarea by page since v0.2.0. Until v0.1.6, it moved the cursor down/up by one paragraph.
  - Add `Ctrl+V` and `Alt+V` keys to scroll down/up the textarea by page as Emacs-like key mappings.
  - Add `Alt+]` and `Alt+[` keys to move the cursor down/up by one paragraph as Emacs-like key mappings.
- **BREAKING** Add `#[non_exhaustive]` attribute to [`CursorMove` enum](https://docs.rs/tui-textarea/latest/tui_textarea/enum.CursorMove.html). This is because more cursor move variations may be added in the future.
- Fix panic when the max history size is zero (which means the edit history is disabled). ([#4](https://github.com/rhysd/tui-textarea/issues/4))

[Changes][v0.2.0]


<a id="v0.1.6"></a>
# [v0.1.6](https://github.com/rhysd/tui-textarea/releases/tag/v0.1.6) - 2022-09-28

- Support mouse scroll. ([#2](https://github.com/rhysd/tui-textarea/issues/2))
  - Handle mouse events for both `crossterm` and `termion` backends.
  - `TextArea::scroll` method was added.
  - `Key::MouseScrollUp` and `Key::MouseScrollDown` virtual keys are added to `Key` enum so that custom backends can support mouse scrolling.
  - `CursorMove::InViewport` variant was added to `CursorMove` enum, which ensures  the cursor to be within the viewport.
- Add `TextArea::alignment` and `TextArea::set_alignment` to set the text alignment of textarea. Note that right and center alignments don't work well with line number so calling `TextArea::set_alignment` with them automatically disables it. ([#3](https://github.com/rhysd/tui-textarea/issues/3), thanks [@Volkalex28](https://github.com/Volkalex28))
  <img src="https://user-images.githubusercontent.com/823277/192801738-4b9d7a18-e282-4c6c-af73-65a94cd8a721.gif" width=590 height=188>
- Set [`rust-version`](https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field) to 1.56.1 in `Cargo.toml` to show MSRV explicitly.

[Changes][v0.1.6]


<a id="v0.1.5"></a>
# [v0.1.5](https://github.com/rhysd/tui-textarea/releases/tag/v0.1.5) - 2022-07-18

- Improve performance to render a textarea widget. When number of lines increases, now rendering lines is **about 2~8x faster** according to [our benchmark suites](https://github.com/rhysd/tui-textarea/tree/main/bench). See [the commit](https://github.com/rhysd/tui-textarea/commit/4e5b684baf4401337bb2e30fd663fa967321f1c1) for more details of the benchmark results. This was archived by managing a vertical scroll position by ourselves instead of scroll handling by `Paragraph`. Previously, a cost of rendering lines was `O(n)` where `n` was number of all lines. Now the cost is `O(1)`.
- Implement `Clone` for `TextArea` so that textarea instances can be copied easily. It is useful when you create multiple textarea instances with the same configuration. Create a first `TextArea` instance with configuring blocks and styles, then simply clone it.
- Add `arbitrary` feature which is disabled by default. By enabling it, `Input`, `Key` and `CursorMove` can be randomly generated via [arbitrary](https://crates.io/crates/arbitrary) crate. This feature aims to be used by fuzzing tests.
- Add many benchmark suites to track performance; insert/delete lines/characters, text search, moving a cursor.
- Improve fuzzing tests to include rendering a textarea to a dummy terminal backend and moving a cursor randomly.
- Refactor `TextArea` implementation. The implementation of text search was separated to `src/search.rs`. The implementation of highlighting was separated to `src/highlight.rs`. And the implementation of widget rendered by tui-rs was separated to `src/widget.rs`. These refactorings changed no public API.

[Changes][v0.1.5]


<a id="v0.1.4"></a>
# [v0.1.4](https://github.com/rhysd/tui-textarea/releases/tag/v0.1.4) - 2022-07-10

- Fix the cursor line style was not applied when a cursor is at the end of line.
- Fix the cursor position after undoing the modification by 'delete until head of line' (`^J` by default).

[Changes][v0.1.4]


<a id="v0.1.3"></a>
# [v0.1.3](https://github.com/rhysd/tui-textarea/releases/tag/v0.1.3) - 2022-07-08

- Text search was implemented. Text search is gated behind `search` feature flag to avoid depending on `regex` crate until it is necessary. See [the usage document](https://github.com/rhysd/tui-textarea#text-search-with-regular-expressions), [the API document](https://docs.rs/tui-textarea/latest/tui_textarea/struct.TextArea.html), and [the working example](https://github.com/rhysd/tui-textarea/blob/main/examples/editor.rs) for more details.
  - `TextArea::set_search_pattern` sets a search pattern in regular expression. This updates highlights at matches in textarea, but does not move the cursor.
  - `TextArea::search_forward` moves cursor to the next match of the text search based on current cursor position.
  - `TextArea::search_back` moves cursor to the previous match of the text search based on current cursor position.
  - `TextArea::set_search_style` sets the text style for highlighting matches of text search.
  <img src="https://user-images.githubusercontent.com/823277/177961514-f63e65de-a562-46d9-b858-4c19e55f8772.gif" width=546 height=156 alt="search in editor example">

[Changes][v0.1.3]


<a id="v0.1.2"></a>
# [v0.1.2](https://github.com/rhysd/tui-textarea/releases/tag/v0.1.2) - 2022-06-25

- Indent with hard tab is now supported. `TextArea::set_hard_tab_indent` method enables indentation with a hard tab on hitting a tab key.
  ```rust
  let mut textarea = TextArea::default();

  textarea.set_hard_tab_indent(true);
  textarea.insert_tab();
  assert_eq!(textarea.lines(), ["\t"]);
  ```
  Demo with `cargo run --example editor`:
  <img src="https://user-images.githubusercontent.com/823277/175755458-5bf60e84-e01f-410d-9194-d3117031eff6.gif" alt="screencast" width=452 height=140>
- Add `TextArea::indent` method to get an indent string of textarea.
  ```rust
  let mut textarea = TextArea::default();

  assert_eq!(textarea.indent(), "    ");
  textarea.set_tab_length(2);
  assert_eq!(textarea.indent(), "  ");
  textarea.set_hard_tab_indent(true);
  assert_eq!(textarea.indent(), "\t");
  ```

[Changes][v0.1.2]


<a id="v0.1.1"></a>
# [v0.1.1](https://github.com/rhysd/tui-textarea/releases/tag/v0.1.1) - 2022-06-21

- Add `TextArea::yank_text` and `TextArea::set_yank_text` to set/get yanked text of the textarea.
  ```rust
  let mut textarea = TextArea::default();
  textarea.set_yank_text("hello, world");
  assert_eq!(textarea.yank_text(), "hello, world");
  textarea.paste();
  assert_eq!(textarea.lines(), ["hello, world"]);
  ```
- Add `CursorMove::Jump(row, col)` variant to move cursor to arbitrary (row, col) position with `TextArea::move_cursor`.
  ```rust
  let mut textarea = TextArea::from(["aaaa", "bbbb"]);
  textarea.move_cursor(CursorMove::Jump(1, 2));
  assert_eq!(textarea.cursor(), (1, 2));
  ```
- Fix hard tabs are not rendered ([#1](https://github.com/rhysd/tui-textarea/issues/1))

[Changes][v0.1.1]


<a id="v0.1.0"></a>
# [v0.1.0](https://github.com/rhysd/tui-textarea/releases/tag/v0.1.0) - 2022-06-19

First release :tada:

- GitHub repository: https://github.com/rhysd/tui-textarea
- crates.io: https://crates.io/crates/tui-textarea
- docs.rs: https://docs.rs/tui-textarea/latest/tui_textarea/

[Changes][v0.1.0]


[v0.7.0]: https://github.com/rhysd/tui-textarea/compare/v0.6.1...v0.7.0
[v0.6.1]: https://github.com/rhysd/tui-textarea/compare/v0.6.0...v0.6.1
[v0.6.0]: https://github.com/rhysd/tui-textarea/compare/v0.5.3...v0.6.0
[v0.5.3]: https://github.com/rhysd/tui-textarea/compare/v0.5.2...v0.5.3
[v0.5.2]: https://github.com/rhysd/tui-textarea/compare/v0.5.1...v0.5.2
[v0.5.1]: https://github.com/rhysd/tui-textarea/compare/v0.5.0...v0.5.1
[v0.5.0]: https://github.com/rhysd/tui-textarea/compare/v0.4.0...v0.5.0
[v0.4.0]: https://github.com/rhysd/tui-textarea/compare/v0.3.1...v0.4.0
[v0.3.1]: https://github.com/rhysd/tui-textarea/compare/v0.3.0...v0.3.1
[v0.3.0]: https://github.com/rhysd/tui-textarea/compare/v0.2.4...v0.3.0
[v0.2.4]: https://github.com/rhysd/tui-textarea/compare/v0.2.3...v0.2.4
[v0.2.3]: https://github.com/rhysd/tui-textarea/compare/v0.2.2...v0.2.3
[v0.2.2]: https://github.com/rhysd/tui-textarea/compare/v0.2.1...v0.2.2
[v0.2.1]: https://github.com/rhysd/tui-textarea/compare/v0.2.0...v0.2.1
[v0.2.0]: https://github.com/rhysd/tui-textarea/compare/v0.1.6...v0.2.0
[v0.1.6]: https://github.com/rhysd/tui-textarea/compare/v0.1.5...v0.1.6
[v0.1.5]: https://github.com/rhysd/tui-textarea/compare/v0.1.4...v0.1.5
[v0.1.4]: https://github.com/rhysd/tui-textarea/compare/v0.1.3...v0.1.4
[v0.1.3]: https://github.com/rhysd/tui-textarea/compare/v0.1.2...v0.1.3
[v0.1.2]: https://github.com/rhysd/tui-textarea/compare/v0.1.1...v0.1.2
[v0.1.1]: https://github.com/rhysd/tui-textarea/compare/v0.1.0...v0.1.1
[v0.1.0]: https://github.com/rhysd/tui-textarea/tree/v0.1.0

<!-- Generated by https://github.com/rhysd/changelog-from-release v3.8.0 -->

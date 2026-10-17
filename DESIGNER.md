# Designer-equivalent widget plugins

Qt Designer plugins in upstream caQtDM are Qt `.so` binaries. This runtime does not load those binaries. A display file is still a `.ui` document: every property is stored, and a class the runtime already knows is drawn as that widget.

A class that is not in the built-in list can be registered before the display is opened. The registry is the stand-in for Designer's custom-widget plugin:

```rust
use caqtdm_widgets::{install_designer_plugin, DesignerKind, DesignerPlugin};

install_designer_plugin(DesignerPlugin {
    class_name: "psiDemoLamp".into(),
    group: "PSI".into(),
    properties: vec!["channel".into(), "foreground".into()],
    kind: DesignerKind::Led,
});
```

`class_name` is the `.ui` widget class. `group` and `properties` are the catalog Designer would have shown. `kind` is which built-in visual the class paints as: `Label`, `Led`, `Frame`, or `Gauge`. A later registration with the same name replaces the earlier one.

`designer_plugins()` returns the catalog. Unknown classes with no registration stay `VisualKind::Unknown` and are not given a stand-in widget.

This is not a loader for `libqtcontrols_monitors_plugin.so`. The paint path for a registered class is covered by `designer_plugin_paints_as_the_registered_kind`.

## Editing a display file

`DisplayEditor` is the stand-in for saving a file. It is not Qt Designer, and Qt Designer is not in the upstream tree. The caQtDM viewer does not write `.ui` either.

```rust
use caqtdm_ui::DisplayEditor;

let mut editor = DisplayEditor::from_bytes(&bytes).unwrap();
assert_eq!(editor.save(), bytes);
editor.set_ui_string("calabel", "text", "kept").unwrap();
```

An unedited `save` returns the input bytes. `set_ui_string` replaces the `<string>` of one property on one widget and leaves the bytes before and after that value. `set_adl_field` does the same for an ADL `field="..."` attribute. Upstream `caQtDM_Tests/test.ui` round-trips, and an inline MEDM file does too. A full Qt Designer resave is a different writer and is not reproduced.

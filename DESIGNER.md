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

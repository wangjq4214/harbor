# Bundled Lucide artwork

This directory contains Harbor's built-in icon subset: `terminal`, `plus`,
`x`, `dot`, `chevron-up`, and `chevron-down`. The original four SVGs and LICENSE
were copied from [Lucide revision dab294f313897bacaa968b725b3259fa37c17bd1](https://github.com/lucide-icons/lucide/tree/dab294f313897bacaa968b725b3259fa37c17bd1).
The chevrons use equivalent Lucide 24-unit outline path definitions.

Keep [LICENSE](./LICENSE) with source and binary distributions containing this
artwork. It includes the Lucide ISC notice and the Feather MIT notice applicable
to `terminal`, `plus`, `x`, `chevron-up`, and `chevron-down`.

Harbor precompiles this simple SVG subset into signed-distance functions in
`src/renderer/quad.rs`; it does not parse arbitrary SVG at runtime. The 24-unit
viewBox, 2-unit strokes, and round caps/joins are preserved. `dot` is presented
as a filled disk with radius 2, filling the original stroked circle's
zero-radius center. Changes to the SVG subset must be reflected in the shader
and its pixel tests. Each icon is one retained `Primitive::Icon`, so translucent
stroke intersections use one coverage value rather than overlapping draws.

```rust
use harbor_widget::{Icon, LucideIcon};
use harbor_widget::scene::primitive::Color;

let icon = Icon::new(LucideIcon::Terminal)
    .size(20.0).expect("finite nonnegative size")
    .foreground(Color::WHITE).expect("finite RGBA");
```

`Icon` is a non-interactive leaf compatible with `view!`. Its default size is
20dp and its default tint is inherited from the theme's normal icon-button
foreground. Compose it inside `InteractiveRegion` for a button or other
state-aware control. It does not require an icon font or JavaScript package.

# UI 描述宏与 Widget 基础 API 审阅及改进建议

## 1. 结论

建议做一轮 **API v2**，而不只是更换宏语法。

目前的复杂性主要来自构建生命周期、子节点协议、回调模型和布局组合不一致。建议方向是：

> 统一构建语义，保留普通 Rust 表达式；明确组件的子节点契约；用少量组合方法降低嵌套。

本文是源码审阅和设计建议。允许讨论破坏性变更。**状态更新（2026-10-23）：F1、F2 已实施并通过各自记录的自动化验证；F3 已实施并通过自动化验证及 native hidden-host smoke，但原生 IME 与实际 HMR DLL 卸载验收仍待完成，见第 9 节；F4–F8 仍为待实施建议。** 第 2–7 节保留原审阅基线与建议，不代表实施后的全部现状；F3 原建议中的 `on_activate` / `enabled` 命名方向已被后续决策修订。

## 2. 审阅基线与范围

- 源码版本：`99d4bc06d75976153e3fc70b470a777d461091ee`。
- 审阅开始和结束时工作树均干净；本文是在审阅结束后新增的报告文件。
- 模式：分析与建议，不实施重构。
- 覆盖：
  - `crates/harbor-widget-macros/src/` 的 4 个生产源码文件。
  - `crates/harbor-widget/src/widgets/` 的全部 31 个生产源码文件，含模块导出文件。
  - `construction.rs`、`lib.rs` 和 `view.rs` 的公开构建 API。
  - 与构建身份、hooks、主题相关的 reconciliation/runtime 接口。
  - tabs、search、appearance 等应用调用，以及相关宏、child construction 测试和文档。
- 测试采用代表性契约阅读，未完整审计所有测试实现。
- 不纳入全面审计：GPU 渲染器、native host、终端引擎和平台适配内部。
- 未运行构建、测试或原生交互验证；下文收益均为设计预期，不是已经实现的结果。

### 当前行为路径

```text
view! 根节点 ── Component::build(cx) ── concrete View
view! 子节点 ── IntoChildView ── deferred View ── Fiber 内构建
已有 View 子节点 ── 原样传递

父节点 child block ── Children ── WithChildren
                                 └─ 错误由宏转成 panic
```

这个路径说明：表面相似的节点声明，并不总有相同的构建时机和状态归属。

## 3. 主要发现与建议

### F1 / P0：统一根节点、子节点和内容 slot 的构建边界

#### 现状与影响

- 宏根节点直接调用 `Component::build(cx)`。
- 子节点通过 `IntoChildView` 延迟到 reconciliation 时构建。
- 已经构建好的 `View` 可以作为子节点，却不能作为宏根节点。
- `InteractiveRegion::content` 使用所属 region 的 `BuildCx` 执行内容构造函数。

因此，提取 UI 函数或调整嵌套位置时，调用者需要判断 hooks 属于谁，不能把所有 UI 组合都当成等价的树描述。应用的 tab row 甚至使用零 padding 包装，明确隔离 stateful region 的 Fiber。

证据：

- `crates/harbor-widget-macros/src/expand.rs:50–150`
- `crates/harbor-widget/src/construction.rs`：`IntoChildView`
- `crates/harbor-widget/src/widgets/interactive_region.rs:137`
- `crates/harbor-app/src/tab_view_ui.rs`：`tab_row`
- `crates/harbor-widget-macros/tests/ui/fail/view_root.rs`

#### 建议

- `view!` 只构造 UI 描述，不立即执行根组件。
- 根和子节点采用相同的描述转换协议。
- 普通 UI 组合不需要传递 `cx`；只有读取状态、主题等组件逻辑需要它。
- 状态驱动内容 slot 应有明确的组件边界，不能默认借用父组件的 hook 序列。
- 保留显式所有权，不隐式 clone，也不以便利语法掩盖 `'static` 约束。

#### 风险与前置条件

这不是仅修改宏展开就能完成的优化。

当前 `crates/harbor-widget/src/fiber/reconcile.rs:126–130` 对组件返回值调用 `decompose()`，要求它已经是 concrete View；`View::decompose` 遇到 deferred View 会 panic。根和子节点统一延迟构建，需要同时明确组件身份、渲染节点与返回树的关系，不能简单替换为 `View::deferred(...)` 或无边界地循环展开。

需要验证：状态归属、key/type 身份、主题继承、外部资源注册、订阅清理、卸载，以及 HMR 旧 generation 回调释放顺序。

### F2 / P0：建立明确且一致的子节点数量契约

#### 现状与影响

| API | 当前行为 |
| --- | --- |
| `Padding::child(a).child(b)` | 后者覆盖前者 |
| `Padding::child(a).with_children(b)` | 返回错误 |
| 宏中的上述错误 | 转成运行时 panic |
| `Align::child(...)` | 可以追加多个 child |
| `Align` 布局 | 只使用第一个 child，并返回一个 placement |
| `SizedBox` 等叶子 | 实现接收空列表、拒绝非空列表的 `WithChildren` |

证据：

- `crates/harbor-widget/src/construction.rs:89–110`
- `crates/harbor-widget/src/widgets/padding.rs:39–58`
- `crates/harbor-widget/src/widgets/align.rs:45–106`
- `crates/harbor-widget-macros/src/expand.rs:82,150`
- `crates/harbor-widget/tests/child_construction.rs`

#### 建议

明确三类组件，并以共享协议表达，而不是让宏维护 widget 名称白名单：

- **Leaf**：不能附加子节点。
- **SingleChild**：一个内容槽。
- **MultiChild**：有序子节点列表。

具体规则：

- 单槽统一用 `.child(value)`，作为属性设置；重复设置统一覆盖。
- 多槽使用 `.children(...)`，追加操作采用明确的方法名，并固定顺序语义。
- 宏和手写 API 使用同一个附加协议，不能一边覆盖、一边报错。
- `Align` 收紧为单子节点；多个内容叠放交给 `Stack`。
- 静态可判断的非法 child 数量尽量编译期报错；动态数量仍需明确检查。
- 删除叶子组件仅用于处理 child list 的样板协议实现。

这是有意的契约变更，必须迁移原先依赖错误或覆盖行为的调用和测试。

### F3 / P0：统一普通 UI 回调的线程模型和事件命名

> 后续实施说明：本节保留原审阅建议。实际契约保留 Button/IconButton 的 `.on_click(...)` 和 InteractiveRegion 的 `.on_activate(...)`，并统一普通控件为 `.disabled(bool)`；见 ADR 0081、0082、spec 0036 及第 9 节 F3 记录。

#### 现状与影响

- `Button::on_click`、`IconButton::on_click`、`MouseRegion::on_enter` 要求 `Send + Sync`。
- `InteractiveRegion::on_activate`、`TextField::on_changed` 没有该要求。
- `Signal` 使用 UI 线程上的 `Rc<RefCell<_>>`。

因此，相同的局部状态更新逻辑，换用 Button 后就不能直接捕获该状态。`docs/widget/learn.md` 需要专门解释这个限制。

证据：

- `crates/harbor-widget/src/widgets/button.rs:27`
- `crates/harbor-widget/src/widgets/icon_button.rs:46`
- `crates/harbor-widget/src/widgets/mouse_region.rs`
- `crates/harbor-widget/src/widgets/interactive_region.rs:119`
- `crates/harbor-widget/src/widgets/text_field.rs`
- `crates/harbor-widget/src/signal.rs`

#### 建议

- 普通 UI 回调统一为 `Fn(...) + 'static`，内部使用 `Rc`。
- `Send + Sync` 只保留在确实跨线程的接口上，不机械修改外部渲染、调度或宿主接口。
- 业务命令仍推荐使用 `Dispatcher`，但它不应成为按钮更新局部 UI 状态的唯一方式。
- 激活统一叫 `.on_activate(...)`，覆盖鼠标、Enter、Space，而非仅 click。
- 状态开关统一为 `.enabled(bool)`，避免同类控件正反命名混用。
- 有焦点能力的控件统一暴露 `.focus_handle(...)`。

需保留事件路由、嵌套控件优先处理、pointer capture 和恰好一次激活的语义。

### F4 / P1：让 Button 支持组合，收敛内容测量与绘制责任

#### 现状与影响

- Button 是固定文字叶子，缺少 IconButton 已有的部分配置能力。
- InteractiveRegion 不仅处理交互，还测量和绘制 label。
- 其 label 宽度按字符数估算，而 TextLabel 已使用 renderer advances 测量。

不同控件重复承担内容展示责任，既限制组合，也增加行为不一致的机会。

证据：

- `crates/harbor-widget/src/widgets/button.rs`
- `crates/harbor-widget/src/widgets/icon_button.rs`
- `crates/harbor-widget/src/widgets/interactive_region.rs:228–335`
- `crates/harbor-widget/src/widgets/text_label.rs`

#### 建议

分清职责：

- InteractiveRegion：交互状态、激活、焦点、capture。
- 控件表面：背景、边框、内边距和焦点环。
- TextLabel / Icon：内容测量与绘制。
- Button：组合上述能力，支持自定义内容。

提供少量便利构造：

```rust
Button::text("Save")
Button::icon(LucideIcon::Plus, "New")
Button::new().child(custom_content)
```

IconButton 可以作为便利组件保留，但需共享行为和配置，不能独立演化。文字测量收敛可能改变既有尺寸，应使用真实布局与视觉样例验证，不视为纯机械搬迁。

保留现有共享交互状态机的价值，不为 API 统一复制输入处理，也不强制为每种责任增加一个公开类型或额外 Fiber。

### F5 / P1：通过少量组合扩展方法降低布局嵌套

#### 现状与影响

应用中固定宽度需要同时设置 min/max，常见的 padding、对齐、装饰需要多层 child 包装。

证据：

- `crates/harbor-app/src/workspace.rs`：`search_bar`
- `crates/harbor-app/src/tab_view_ui.rs`：`tab_rail`、`close_tab_button`
- `crates/harbor-app/src/appearance.rs`：`GlassAppearance::panel`

#### 建议

在 `ComponentExt` 上优先提供：

- `padding`
- `width` / `height` / `size`
- `constraints`
- `align`
- `decorated`
- `expanded`
- `keyed`

例如：

```rust
content
    .padding(Insets::all(8.0))
    .width(440.0)
    .align(Alignment2D::TOP_RIGHT)
```

这些方法应是已有组合机制的便利入口，而不是给所有 widget 增加万能 style 对象。

必须明确：

- 方法链顺序表示从内到外的包装顺序。
- padding、clip、背景、命中区域不能随意合并或重排。
- `.expanded()` 必须满足直接父级消费 flex metadata 的现有契约。
- 包装不得意外丢失 key、状态或增加焦点停靠点。
- 区分控件内部 padding 和外部包装。当前 `TextField::padding` 已有内部含义，不能让同名扩展方法产生不同语义。

### F6 / P1：统一构造器、导入入口和配置命名

#### 现状与影响

当前同时存在：

```rust
Focus::empty()
Focus::new(child)
Shortcuts::empty()
Shortcuts::new(child)
Actions::handler(handler)
Actions::new(child, handler)
```

部分常用类型从 crate 根导出，Padding、Align、Stack、TextLabel 等则需要深入模块导入。

证据：

- `crates/harbor-widget/src/widgets/{focus,shortcuts,actions}.rs`
- `crates/harbor-widget/src/lib.rs`
- `crates/harbor-widget/src/widgets/mod.rs`
- `docs/widget/catalog.md`

#### 建议

- 构造器只接收必要配置，不接收 child。
- child 经统一 slot API 添加。
- 可选行为使用 builder 方法。
- 增加受控的 `harbor_widget::prelude::*`，覆盖日常组件、Component、View 和扩展 trait。
- 根导出与模块导出形成明确规则，不靠调用者猜测。

不建议为了名称更短而把全部类型重新命名；重命名应解决具体歧义。

### F7 / P1：收敛数值校验与错误暴露策略

#### 现状与影响

非法数值的处理可能是：

| 入口 | 行为 |
| --- | --- |
| `Icon::size` | 忽略非法值 |
| `TextField::padding` | 非法值归零 |
| `BackdropFilter::new` | 构造时返回错误 |
| `Flex::gap`、`Separator::thickness` | 布局时报错 |

调用者难以从 API 外观判断错误何时暴露。

#### 建议

- 配置输入在边界通过 `try_*` 或校验类型处理。
- widget 尽可能接收已验证配置。
- 依赖父约束才能判断的问题继续在布局阶段诊断。
- 不再随意静默忽略或归零。
- 不为链式调用美观而把所有错误改成 panic。
- 共享校验规则，但不为每一个数字创建独立类型。

这会改变非法输入的可观察行为，需明确迁移规则，并补充 NaN、无穷、负值、溢出等测试。

### F8 / P2：用具名类型替换容易误用的位置参数

建议：

| 当前接口 | 建议 |
| --- | --- |
| 四个 `f32` 的 padding/insets | `Insets` |
| Align 的一维 Alignment 加 vertical override | `Alignment2D` |
| `on_vertical_navigation(bool)` | `NavigationDirection` |
| material 的 `[f32; 4]`、`[f32; 2]` | 具名 tone/rim 配置 |

证据：

- `crates/harbor-widget/src/widgets/padding.rs:18`
- `crates/harbor-widget/src/widgets/align.rs`
- `crates/harbor-widget/src/layout/align.rs`
- `crates/harbor-widget/src/widgets/editable_text.rs`
- `crates/harbor-widget/src/widgets/backdrop_filter.rs:58–106`

`Alignment::Stretch` 不应出现在只定位、不实际拉伸的 API 中。类型应阻止具体误用，而不是增加没有约束价值的包装。

## 4. 宏的目标方向

保留现在的主体：

- 普通 Rust 表达式。
- `=>` 表达子树。
- `for`、`if`、`match` 组织子节点。
- 显式所有权，无隐式 clone。
- 解析/展开与运行时分离。

不建议同时引入 JSX、独立属性语言或宏内部状态系统，否则会把 API 的问题转成另一套 DSL 的复杂性。

优先改造：

1. 去掉根节点立即构建的特殊规则。
2. 去掉必须传递 `cx` 的要求。
3. 统一根与子节点接受的描述类型。
4. 尽早暴露静态错误，诊断指向具体节点。
5. 保留表达式求值一次、兄弟顺序、分支惰性执行等保证。
6. 对目前测试固定的 children-before-parent 构造顺序做显式处置，不借重构无意改变副作用顺序。

以下是目标 API 草图，**不是当前可编译代码**：

```rust
use harbor_widget::prelude::*;

view! {
    Column::new().gap(8.0) => {
        TextLabel::new("Search");

        Row::new().gap(4.0).cross_align(CrossAlign::Center) => {
            TextField::new(controller.clone())
                .enabled(enabled)
                .expanded();

            Button::icon(LucideIcon::X, "Close")
                .on_activate(move |_| actions.dispatch(Action::Close));
        }
    }
}
.padding(Insets::all(8.0))
.width(440.0)
```

目标不是少几个符号，而是让调用者不再手动安排 `.build(cx)`、类型转换和临时布局包装。

## 5. 依赖有序的实施单元

下列是建议计划，不是本次已执行工作。开始实施前需要确定最终公开签名和构建模型。

### U1：固定基础契约

- 对应：F1、F2、F3、F6、F7、F8 的共享约定。
- 写入所有权：`construction.rs`、公开 API 契约/测试和相关设计文档。
- 读取依赖：宏、View、reconciler、widgets、现有消费者。
- 产出：slot 分类、key 身份、构建边界、回调约束、命名与校验策略。
- 保留：有序子节点、显式所有权、业务与 widget 边界。
- 验证：先固定状态、主题、child 数量、求值顺序、卸载的 characterization 场景。
- 预期收益：后续迁移不再分别定义共享语义。

### U2：统一控件与子节点 API

- 对应：F2、F3、F4、F6。
- 前置：U1。
- 写入所有权：基础 widgets、construction 协议实现、相关消费者和测试。
- 读取依赖：event routing、focus/capture、主题与文本测量接口。
- 保留：嵌套控件优先处理、恰好一次激活、disabled/focus、滚动边界传递。
- 验证：局部 Signal 可被 UI 回调捕获；鼠标/键盘激活一致；关闭子按钮不激活父行；macro/builder child 语义一致。
- 预期收益：同类控件可以互换，减少样板协议与重复内容展示路径。

### U3：组合方法与具名配置

- 对应：F5、F7、F8。
- 前置：U1；依赖 U2 已稳定的 child 接口。
- 写入所有权：`ComponentExt`、布局 wrapper、配置类型及相关调用。
- 读取依赖：父约束、flex parent data、clip/paint/hit-test 契约。
- 保留：包装顺序、父约束权威、key/state、命中区域与焦点数量。
- 验证：便利方法与显式 wrapper 的几何/场景/事件等价；非法输入策略一致。
- 预期收益：降低常见布局深度和参数误用。

### U4：统一构建链与宏

- 对应：F1 和宏改造。
- 前置：U1、U2；宏使用的共享协议须先稳定。
- 写入所有权：`view.rs`、`fiber/reconcile.rs`、相关 runtime 构建路径、宏的 AST/parse/expand 和契约测试。
- 读取依赖：外部注册、hook/subscription 生命周期、主题、根替换、HMR。
- 集成方式：作为关联单元串行处理共享文件，不能仅合并宏改动。
- 保留：身份与状态、订阅清理、主题继承、外部回调注册与卸载顺序。
- 验证：根/子节点同义；状态独立；keyed 重排保状态和焦点；返回树支持所选描述形式；求值顺序和宏卫生。
- 预期收益：UI 提取和组合不再要求调用者理解 eager/deferred 的位置差异。

### U5：应用迁移、文档与旧路径移除

- 对应：全部发现的消费者迁移和收尾。
- 前置：相关基础接口稳定；最终验收在 U2–U4 集成树上执行。
- 写入所有权：tabs、search、palette、confirmation、appearance、示例与开发者文档。
- 读取依赖：业务命令边界、终端桥接、native host/HMR 契约。
- 保留：业务状态与效果顺序、终端/session 身份、焦点、IME、滚动和确认权限。
- 验证：实际调用可读性、受影响构建、交互测试、原生窗口与支持的 HMR 场景。
- 预期收益：仓库只维护一套新契约，不长期保留双 DSL、旧入口或无价值兼容层。

## 6. 验证要求

实施时至少需要覆盖：

- 宏编译通过/失败诊断，包括跨 crate 和依赖重命名。
- 解析与展开卫生、求值次数、顺序、分支可达性、所有权移动。
- macro 与手写 builder 的结构、布局、场景与事件等价。
- keyed 插入、删除、重排后的状态和焦点保留。
- 内容 slot 的状态独立性与主题继承。
- 单子节点、叶子、多子节点规则及动态数量错误。
- Button/IconButton/自定义内容的激活、disabled、焦点环、capture、嵌套关闭行为。
- 组合方法的 padding/clip/paint/hit-test 顺序与 flex metadata。
- 文本测量变化对长标题、多语言文字和窄空间的影响。
- 外部 draw/schedule/IME 注册与 unmount 清理。
- 原生输入、剪贴板、IME、终端嵌入与 HMR teardown。

可从现有 `macro_tests.rs`、`tests/child_construction.rs`、宏 crate 的 trybuild 用例及交互/焦点/主题测试扩展。未运行或无法运行的检查必须单列，不能算通过。

## 7. 不建议做的事情

- 只把分号和箭头换成 JSX，而保留原有构建差异。
- 引入万能 Context、万能 Style 或新的全局事件总线。
- 为减少文件数删除共享交互状态机或 widget/application 边界。
- 把所有包装合并成一个组件，隐藏顺序、命中、clip 和 flex 语义。
- 把所有回调都改成跨线程，或把所有错误都改成 panic。
- 将 Harbor 产品玻璃效果、命令意义或终端模型移进通用 widget。
- 为追求 LOC 降幅删除必要测试、生命周期处理或有价值的便利组件。

## 8. 优先级与审阅状态

如果先做三件事：

1. 统一构建边界。
2. 统一 child 契约。
3. 去掉普通 UI 回调的多余线程约束。

建议保留普通 Rust 表达式语法、共享交互状态机、controller/editing-core/field 分层、共享 flex 引擎，以及应用 → widget 的依赖方向；这些边界具有实际价值，不应仅因实现较复杂而删除。

本次审阅已覆盖所选生产 API 范围，未进行全仓库正确性审计。**状态更新（2026-10-23）：F1 已实施并完成所记录的自动化验证（spec 0034 / ADR 0079）；F2 已实施并通过自动化验证（spec 0035 / ADR 0080）；F3 已实施并通过自动化验证及 native hidden-host smoke，仍有原生 IME/HMR 验收缺口（spec 0036 / ADR 0081、0082）；F4–F8 仍为待实施建议。** 审阅当时的结论见上文原文。

原建议中 U1 的构建边界与 child 契约、U2 的 child API、U4 的相关宏改造及对应消费者迁移已随 F1/F2 落地；F3 进一步落实了 U1/U2 的普通 UI 回调、控件命名和焦点转发契约，以及对应 U5 调用与文档迁移。这不代表 U1–U5 整体完成。后续仍需单独推进 F4–F8，并保留 F1/F3 记录中的人工集成验证待办。每个阶段应独立验证，避免同时改变构建、布局和事件行为而无法定位回归。

## 9. 实施状态记录(2026-10-23)

### F1:已实施并验证

- **产物**:`.grimoire/spec/0034-deferred-view-root-and-content-slot-boundary.md`(In Progress)、`.grimoire/adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md`(Testing,部分取代 ADR 0032 的根构建条款)、`.grimoire/plans/0045-deferred-view-root-and-content-slot.md`。
- **已落地**(一次性迁移,共 54 个文件):
  - 宏:`view! { root }`,不再传 `cx`;根与子节点统一经 `IntoChildView` 转换,已构建 `View` 可作为根;旧签名以聚焦诊断拒绝(trybuild 用例覆盖)。
  - 运行时:`Runtime::set_root` 放宽为接受 `IntoChildView`(组件或 `view!` 描述,向后兼容);根 Fiber 本就由 Runtime 的 `set_root`/`rebuild_root` 持有,未重设计;新增 `materialize_for_fiber`(见下面的契约修订)。
  - 内容 slot:`InteractiveRegion::content` 经内部 `ContentSlot` 组件在独立子 Fiber 构建——hooks/订阅归 slot,主题继承,稳定 key `harbor-content-slot`;移除 tab row 的零 padding 隔离包装并验证等价。
  - 文档:`docs/widget/learn.md`、`concepts.md`、`crates/harbor-widget/README.md` 已同步。
- **验证**:`cargo test --workspace` 56/56 全绿(运行时等价、keyed 重排、事件路由、trybuild、slot 隔离/卸载、求值顺序);`cargo fmt`;workspace 零警告。原生窗口、IME、HMR 实机会话未在本环境运行,人工集成验证通过后 ADR 0079 → Completed、spec 0034 → Completed。

### F1：相对本文建议的契约修订

- F1"风险与前置条件"指出的"组件返回描述与 Fiber 身份关系"已在实施中定为:**build 返回的 deferred 描述在同一 Fiber 上有界原地物化**(深度上限 8,超限报错而非死循环),取代"deferred build 返回 panic"的初稿措辞。这保持"无第二描述类型"和"无边界循环展开仍禁止"的约定;修订已写回 ADR 0079 D3、spec 0034 R7 与 CONTEXT 的 Deferred Root 条目。
- 宏根 Fiber 的 TypeId 现在等于挂载描述组件的类型,而非展开后的具体根组件类型;布局/场景/事件输出已由等价测试确认不变。

### F2：已实施并通过自动化验证

- **产物**：`.grimoire/spec/0035-unified-child-cardinality-and-attachment.md`、`.grimoire/adr/0080-unify-child-cardinality-and-replacement-semantics.md`、`.grimoire/plans/0046-unified-child-cardinality-and-attachment.md`（含 R1–R7 验收证据）。本次仅同步本文状态，未修改关联 spec/ADR 的状态字段。
- **已落地**：
  - 公共协议：`WithChildren::Capability` 由组件声明 `SingleChild` / `MultiChild`，`Keyed` 转发能力与 key；叶子不再实现仅用于接收空列表或拒绝子节点的附加协议，无 widget 名称白名单。
  - 设置与追加：单槽 `.child(value)` 覆盖；多槽 `.children(values)` 替换列表，`.push_child(value)` / `.extend_children(values)` 显式追加。宏 block 是一次替换，空 block 清空；普通值表达式保留已有内容。旧多槽 `.child(...)` 调用及相关测试、文档已迁移。
  - 数量诊断：静态数量经公共 const-count 约束检查；`if` / `match` 分支按互斥备选计算，不相加。单槽静态超量和所有叶子 block 在编译期拒绝，诊断定位接收表达式。动态列表只验证本次传入数量，手写路径返回 `ChildConstructionError`，普通 `view!` 在构造处明确 panic，不截断、不留到布局阶段。
  - 真实槽位：`Align` 改为 `Option<View>` 单槽；`InteractiveRegion` 的 child/content 相互替换，空 block 可清空任一选择。原 ContentSlot 独立 Fiber 的 hooks、主题、订阅及卸载边界保持不变。
  - 不变量：保留单次求值、兄弟顺序、children-before-parent、分支惰性、显式所有权及 deferred 构建；覆盖已暂存但被替换表达式的求值，以及 macro/builder 的结构、key、几何、场景与路由等价。
- **最终验证**：
  - `cargo test -p harbor-widget -p harbor-widget-macros -p harbor-app`：1,193 项通过，0 失败；含 19 个 trybuild 编译通过/失败 fixtures 与依赖重命名检查。最初跳过 3 项 opt-in 测试。
  - `cargo test -p harbor-app --lib -- --ignored`：补跑的 2 项 GPU 像素测试均通过，以上两次命令共 1,195 项通过。
  - `cargo check --workspace --all-targets`、`cargo fmt --check`、所有改动 Rust 文件（含 fixtures）的 rustfmt 检查及 `git diff --check` 均通过；仍有既有 unused-import/variable 警告。
  - 已完成实施、测试、风险审阅、需求核对及格式化后的集成复验，无已确认遗留阻塞。
- **未运行与范围限制**：需要交互桌面的 native hidden-host 生命周期 smoke test 未运行；本轮未进行原生窗口、IME 或 HMR 人工会话验证，也未实现 F3–F8。该结果不替代 F1 已记录的人工集成验证待办。

### F3：已实施并通过自动化验证，原生验收仍有缺口

- **产物**：`.grimoire/spec/0036-ui-thread-callbacks-and-control-api-consistency.md`、`.grimoire/adr/0081-preserve-click-naming-and-standardize-disabled-builders.md`、`.grimoire/adr/0082-ui-thread-local-widget-callbacks.md`、`.grimoire/plans/0047-ui-thread-callbacks-and-control-api.md`（含 R1–R5 对照及验证证据）。关联 spec/ADR 状态字段未修改，不据局部验证宣称整体验收完成。
- **相对原建议的修订**：保留 Button/IconButton `.on_click(...)`，涵盖既有鼠标与 Enter/Space 逻辑激活；InteractiveRegion 继续使用 `.on_activate(...)`。可用性统一沿用 `.disabled(bool)`，而非原建议的 `.enabled(bool)`；无关的 Focus `.enabled(...)` 保持不变。
- **已落地**：
  - 普通 UI 回调：Button/IconButton 点击、MouseRegion enter/exit、IconButton `color_with` 和 Notification dismissal 采用 `Rc` 存储与 `Fn(...) + 'static`，可直接捕获 Signal 或其他 UI-local 状态；不改变真实跨线程接口、外部渲染和调度的线程契约。
  - 编辑器：TextField/EditableText 改为 `.disabled(bool)`，默认仍可用；旧 `.enabled(...)` 方法移除且不保留别名。调用按等价极性迁移，read-only 的选择/复制与 disabled 的编辑/焦点限制保持独立。
  - 焦点：Button/IconButton `.focus_handle(FocusHandle)` 直接转发到既有 InteractiveRegion，不添加 Focus 包装、额外 Fiber 或 Tab stop；共享交互状态机及路由、capture、键盘时机保持不变。
  - 上层调用：Notification composition 保留已配置组件，删除额外 Arc 与纯转发回调；逐一检查 tab/search/palette/confirmation，保留具有视觉、generation/source、生命周期或 Host 协调职责的 helper 与同步状态。业务命令仍通过 Dispatcher，由 Host 按原有顺序执行。
  - 文档与覆盖：Learn 增加直接捕获局部 Signal 的按钮示例，保留业务 intent 与 Host drain/wakeup 说明；catalog 和编辑器文档同步。新增公共 API 编译通过/失败 fixtures、局部状态真实控件交互、焦点遍历/恢复、回调替换/移除/根卸载释放及确认权限回归测试。
- **最终验证**：
  - `cargo test -p harbor-widget -p harbor-app -p harbor-widget-macros --all-features`：1,203 项通过，0 失败，3 项忽略；包括公共 API fixtures、HMR lifecycle 单元测试和应用交互回归，不代表真实 DLL 卸载验收。
  - `cargo test -p harbor-widget --features winit --test native_host_smoke -- --ignored --nocapture`：另行补跑上述忽略项中的 hidden-host smoke，1 项通过，覆盖真实隐藏窗口 GPU 呈现及重复次级窗口生命周期。
  - `cargo check --workspace --all-targets --all-features` 通过；文档示例检查（含 crate READMEs）18 项通过；13 个改动/新增 Rust 文件的 rustfmt 检查及 `git diff --check` 通过。既有 unused-variable 警告未作为本次清理范围。
  - 实施、测试、inline 风险审阅与需求核对、格式化后的集成复验均已执行，无已确认代码阻塞。日志位于 `target/spec0036-final-tests.log`、`target/spec0036-final-check.log`、`target/spec0036-native-smoke.log` 和 `target/spec0036-doc-examples.log`。
- **未运行与范围限制**：未运行 live native IME/candidate-window 交互、实际 HMR 旧 generation 回调销毁先于 DLL 卸载的验收，以及本轮忽略的 2 项应用 GPU 像素测试。headless IME/root teardown 与 native host smoke 不替代这些证据；F3 尚不能标记为整体验收完成。F4–F8 未纳入本轮实施。

### 其余发现

- F4–F8 仍为待实施建议；U1 的构建边界、child 契约与普通 UI 回调/命名契约已分别由 ADR 0079、0080、0081/0082 固定。Button 内容组合、布局便利 API、构造器/导出、数值校验与具名配置尚需单独推进；沿用本文件的证据与验证要求，不将 F3 的完成范围扩大到这些发现。

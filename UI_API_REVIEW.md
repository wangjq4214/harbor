# Harbor Widget API 设计复审

## 1. 当前结论与基线

**继续收敛现有 API，不另起笼统的 API v2。U1–U3 的修复代码已落地，待实现单元为 U4–U6；实现状态与完整验收分开记录。**

- 状态同步基线：`3ea728c40719c576914242e1d6050f2a923f8e82`；原源码复审基线：`b5c41bb`。
- 保留 F/U 编号供 ADR/spec 引用；已解决问题仅留摘要，旧探针、审查过程及历史测试成绩不再作为当前待办。
- P1 表示优先处理的契约/行为风险，P2 表示接口收敛或低层入口改进。建议不等于已发生的产品故障，也不等于实施授权。

## 2. 已落地的工作

| 单元 / 发现 | 当前实现 | 回归证据入口 |
| --- | --- | --- |
| U1 / F2：构造能力 | FocusScope、CustomPaint、Portal 已使用 `Option<View>` 和共享 `into_single`；child 替换、空 block 清空，动态数量越界返回错误 | `crates/harbor-widget/tests/{child_attachment,single_child_lifecycle}.rs`；提交 `751e841` |
| U2 / F11、F12：事件消费与提交 | router 按 `EventHandled` 控制传播，移除公开 `stop_propagation()`；无条件完成当前事件提交后再合并 hover 重绘标志 | `crates/harbor-widget/tests/event_consumption.rs`、`crates/harbor-widget/src/runtime/event_router/tests.rs`；提交 `9514fa9` |
| U3 / F10、F16：规范化与内部边界 | KeyChord 字段私有、通过构造器规范化并提供访问器；重复绑定保留 first-match；`View::new` 和 Signal 原始订阅收为 crate-private | `crates/harbor-widget/tests/{key_chord,runtime_dependencies}.rs`、`crates/harbor-widget-macros/tests/ui/{fail,pass}/` 的公开边界 fixtures；提交 `3ea728c` |

更早的 F1/F3/F4/F5 也不再列为待实现：宏与 Runtime 已支持延迟构造和已有 View 根，内容 slot 拥有独立 Fiber，普通 UI 回调支持 UI-local captures，可组合 Button、共享 ControlSurface、显式布局组件与 fallible final-node 宏已存在。原生 Host 根工厂的剩余差异单列 F9。

相关契约：ADR 0079–0084 / spec 0034–0038；U2 见 [ADR 0085](.grimoire/adr/0085-return-driven-event-consumption-and-unconditional-commit.md) / [spec 0039](.grimoire/spec/0039-return-driven-event-consumption-and-commit.md)，U3 见 [ADR 0086](.grimoire/adr/0086-normalized-key-chords-and-private-runtime-control.md) / [spec 0040](.grimoire/spec/0040-normalized-key-chords-and-private-runtime-control.md)。

**验收边界：** U2 的 ADR 仍为 Testing、spec 仍为 Draft；U3 的 ADR/spec 已为 Completed/Implemented。本次不修改这些状态，也不因旧文档中的 spec-only 措辞否认现有实现。上述测试是回归证据入口，不是本次执行结果；原生焦点、IME 和真实 HMR generation teardown 的既有验证义务见第 6 节。

## 3. 剩余问题

以下源码路径除特别注明外均相对 `crates/harbor-widget/src/`。

### F6 / P2：wrapper 构造、导入与命名尚未收敛

**设计建议。** `widgets/{focus,shortcuts,actions}.rs` 仍采用 `empty/new(child)`、`handler/new(child, handler)`，虽实现 WithChildren，却没有普通 `.child(...)` setter。Focus 的 `.handle(...)` 与控件的 `.focus_handle(...)` 不一致。

Padding/Align/SizedBox 已根导出；Stack/TextLabel 仍位于子模块，Component/BuildCx/View 位于 `view` 模块，扩展 trait 在根。应明确推荐导入规则，prelude 仅为可选方案，不是缺陷本身。

**建议：** 构造只接必要配置，补一致的 child setter，统一焦点配置命名；完整映射见[第 7 节](#7-widget-与方法命名收敛清单待实施建议)。不为减少 import 行数导出全部内部类型，也不机械生成 Props。

### F7 / P1：剩余外观配置缺少可预测的验证边界

**静态确认；按领域收敛，不要求所有入口一律返回 Result。**

| 当前入口 | 行为/问题 | 位置 |
| --- | --- | --- |
| `Icon::size` / `IconButton::icon_size` | 非法值保留旧值，调用看似成功 | `widgets/{icon,icon_button}.rs` |
| `TextField::padding` | 负值/非有限值归零，与外部 Padding 策略不同 | `widgets/text_field.rs` |
| `Separator::thickness` | 原样保存，直到 layout 才报错 | `widgets/separator.rs` |
| `ControlStyle`、`Theme::spacing` | 公开原始数值直接进入 region/surface/图文布局，没有统一验证入口 | `theme.rs`、`widgets/{control_surface,button}.rs` |
| `NotificationStyle` / placement inset | style 声明尺寸约束但未验证；inset 也原样接受 | `widgets/notification.rs` |

**建议：** 明确尺寸、颜色、材质上限的合法域和错误策略，再让等价入口共用验证。公开输入结构体不等于已验证配置，消费边界仍需负责。保留“非法 scalar 在边界处理，父约束或有效值组合溢出在 layout 处理”的区分；不重做已迁移的 SizedBox/Padding/Align/Flex gap，也不机械删除 Backdrop 的性能 cap 或 ScrollController 的请求归一化。

### F8 / P2：位置参数与定位类型仍可收紧

**设计建议。**

- `widgets/{editable_text,text_field}.rs`：`on_vertical_navigation(bool)` 的 true 表示 down，应用 palette 依赖隐含约定；建议 `VerticalDirection::{Up, Down}`。
- `widgets/backdrop_filter.rs`：tone `[offset,slope,quadratic,saturation]`、rim `[highlight,shade]` 的参数可对调而不报编译错误；建议具名局部配置及共享验证，GPU 上传仍可用紧凑数组。
- `widgets/notification.rs`：placement 与 close alignment 接收含 Stretch 的 `Alignment`，实际仅定位，Stretch 等同 Start；应使用已有 `PositionAlignment/Alignment2D`。

不重做 Insets/Align，也不删除 Flex 真正使用的 Stretch。

### F9 / P2：原生 Host 根工厂仍拒绝已有 View

**静态确认。** `winit/host.rs` 的 `build/build_with_output` 要求 `R: Component`，而 View 并未实现 Component；`Runtime::set_root` 已接受 IntoChildView。宏 View 或返回 View 的普通组合函数仍需额外包装才能作为 Host 工厂结果。

**建议：** 两个工厂统一接收 `IntoChildView + 'static`，保留工厂错误、bootstrap output 与挂载时机；增加普通组件、宏 View、已有 View 的公共编译契约，不重设计根身份或 HMR。

### F13 / P1：致命 GPU 错误缺少完整生产回传链路

**静态确认接线缺口；真实设备故障表现尚待验证。** `winit/presenter.rs` 公开 OutOfMemory/DeviceLost 等 FrameError，应用 `src/shell.rs` 依赖 HostFrameOutcome::Fatal 决定退出；但 `winit/gpu.rs` 创建设备后未接入 device-lost/uncaptured-error 通知，编码、submit/present 也没有完整错误回传。现有分类主要覆盖 surface acquisition/recovery。

**建议：** SharedGpu 持有设备级错误状态及唤醒机制，在帧/宿主边界转换为明确结果。说明异步报告时机及“提交/请求呈现不等于 GPU 执行完成”，不承诺同步捕获所有错误。注入 FrameError 的策略测试不能替代真实生产接线、多窗口共享设备和设备故障验证。

### F14 / P2：QuadRenderer 公开入口缺少绘制顺序契约

**静态确认，低层 API 改进建议；不表示 Runtime 当前排序错误。** `renderer/quad.rs` 的 update 保留/复用 GPU slot，encode 按 slot 区间而非 SceneItem.paint_order 绘制；`runtime/frame_encoder.rs` 已按场景顺序查 slot 并分段编码。

独立消费者直接 `update(delta) → encode(pass)`，在插入、重排或槽复用后可能产生错误遮挡；稳定存储槽不等于绘制顺序，槽复用测试也不能证明遮挡正确。

**建议：** 推荐 Runtime 的 paint-ordered 入口；将 slot/range 操作收为内部，或明确命名为 storage-order draw 并提供完整 ordered-items 路径。保留内部槽复用和批处理，不重写渲染器。

### F15 / P2：不可绘制窗口忽略 host_deadline

**静态确认；deadline 承诺范围仍需明确。** `winit/host.rs` 接受 host_deadline，经 presenter 交给 scheduler；`winit/scheduler.rs` 的 `!drawable` 分支直接返回 Wait，仅可绘制路径仲裁 runtime/recovery/host deadline。

若调用者借此安排非渲染计时，零尺寸/最小化且无其他事件时就不会按期唤醒。当前应用仍自行仲裁通知等业务 deadline，不能据此断言业务计时已全部失效。

**建议：** 不可绘制时暂停 redraw/GPU 动画，但保留宿主未来 WaitUntil；或将该参数明确限定/命名为渲染计时，由调用者自行仲裁业务 deadline。补零尺寸下有/无未来 deadline、恢复 drawable 的状态测试。

## 4. 待实现单元

| 单元 | 对应发现 / 顺序 | 修改范围与验证重点 |
| --- | --- | --- |
| U4：命名、具名配置与一致入口 | F6、F7、F8；验证策略先于消费者迁移 | 配置所属模块、wrapper/导出、应用、宏 fixtures 与文档；覆盖非法值矩阵、等价布局/样式、父约束和业务默认值，命名清单见第 7 节 |
| U5：贯通宿主与 GPU 结果 | F9、F13、F15；三个子项可独立推进 | host/gpu/presenter/scheduler、main/dialog、多窗口与 HMR 接缝；覆盖组件/View 工厂编译、deadline 仲裁、故障注入与原生验收，保持 Host 所有权、工厂错误和 teardown 顺序 |
| U6：明确 renderer 公开层次 | F14；无强制前置 | renderer 入口/导出、独立消费者和文档；保持内部复用/批处理，验证重排、删除再插入及半透明重叠的 GPU 结果 |

公共契约改变前固定迁移策略；共享文件和互相依赖的迁移串行或原子集成。U1–U3 不再重复实施，但后续变更仍需保留其回归契约。

## 5. 后续修改应保留的边界

- 普通 Rust 表达式宏、单次求值/分支惰性/children-before-parent、显式布局树；不新增 JSX、属性 DSL、通用布局方法链或万能 Container。
- 独立内容 slot Fiber、parent-local key/type、直接 flex parent data、原子布局提交；不承诺任意 wrapper 改写或跨父级移动保状态。
- 共享交互状态机、私有 ControlSurface、Button/IconButton 各自 theme role；保留 controller → editing core → decorated field 分层。
- Store/Dispatcher 的 FIFO 与 Host drain/wakeup 分工；没有自动 wake 是既定契约。
- Host 持有 Window/Surface/Runtime，应用持有 EventLoop、业务、多窗口和退出策略；保留 HMR 代际门控、销毁屏障及原生资源析构顺序。
- 保留有界 Lucide、借用 GPU 能力及应用 glass/命令策略边界；UIA、虚拟列表或完整 shaping 不属于本次 backlog。

## 6. 验证与同步范围

- 本次仅同步本文，对照当前源码、相关 ADR/spec 及回归测试入口；不是新一轮全量源码审计，未修改生产代码、测试或 ADR/spec。
- 本次未重跑代码测试或 workspace 检查，不沿用旧探针、测试数量或缓存警告作为当前验证结果。
- U1 的 mounted 生命周期、U2 的事件消费/提交、U3 的路由/订阅和公开编译边界已有正式回归测试；这不等于全部需求验收通过。
- 原生焦点、IME、真实 HMR DLL 卸载及 generation teardown 的既有证据缺口继续保留；mounted/headless 测试不能替代这些验收。
- U5/F13 仍需设备错误生产链路、故障注入、多窗口与真实设备验证；U6/F14 仍需独立入口绘制顺序及 GPU 遮挡验证。

## 7. Widget 与方法命名收敛清单（待实施建议）

本节归入 U4 / F6，并与 F7/F8 的验证及配置迁移配套。**以下是建议目标名，不是当前可用 API、最终公开签名或已实施结果。** 不因名称不够整齐就重写组件职责；类型名只改职责不准确的，方法名重点统一设置、查询、动态配置、事件与追加的规则。

### 7.1 Widget 和相关类型名

| 当前名称 | 建议名称 | 理由 |
| --- | --- | --- |
| `CustomPaint` | `ExternalSurface` | 实际承担外部绘制、输入、IME 和调度，不只是自定义绘画 |
| `Notification` | `NotificationCard` | 表达通知卡片展示，不暗示通知生命周期管理 |
| `NotificationStyle` | `NotificationCardStyle` | 跟随组件名称 |
| `NotificationPlacement` | `NotificationCardPlacement` | 跟随组件名称；定位类型收紧仍按 F8 处理 |
| `Actions<A>` | `ActionHandler<A>` | 提供动作处理函数，不是动作集合 |
| `Shortcuts<A>` | `ShortcutMap<A>` | 明确是快捷键到动作的映射 |
| `Alignment` | `CrossAxisAlignment` | 含 Stretch 的交叉轴布局能力，与纯定位类型区分 |

`Alignment` 的迁移须先把 Notification 等仅定位的消费者改用 PositionAlignment/Alignment2D；不能把所有引用机械替换后继续在定位入口暴露 Stretch。MainAxisAlignment、PositionAlignment、Alignment2D、FlexOptions、FocusHandle 和各 Controller 名称保留，不另造同义类型。

建议保留的组件名：

- 布局：`Row`、`Column`、`Flex`、`Stack`、`Padding`、`Align`、`SizedBox`、`ConstrainedBox`、`Flexible`、`Expanded`、`Spacer`。
- 内容与控件：`TextLabel`、`Icon`、`Button`、`IconButton`、`EditableText`、`TextField`、`Separator`。
- 包装与基础机制：`DecoratedBox`、`BackdropFilter`、`ScrollView`、`Focus`、`FocusScope`、`MouseRegion`、`InteractiveRegion`、`LayoutObserver`、`ThemeProvider`、`Portal`。

这些名称的职责差异有实际价值，不统一添加 Widget/Box 后缀，不合并 TextLabel、EditableText、TextField，也不把 Portal 改成暗示独立原生窗口的名称。私有 ControlSurface 继续作为内部实现。

### 7.2 构造器与内容方法

| 当前 API | 建议 API |
| --- | --- |
| `Focus::empty()` | `Focus::new()` |
| `Focus::new(child)` | `Focus::new().child(child)` |
| `Shortcuts::empty()` | `ShortcutMap::new()` |
| `Shortcuts::new(child)` | `ShortcutMap::new().child(child)` |
| `Actions::handler(handler)` | `ActionHandler::new(handler)` |
| `Actions::new(child, handler)` | `ActionHandler::new(handler).child(child)` |
| `Button::content(builder)` | `Button::child_with(builder)` |
| `InteractiveRegion::content(builder)` | `InteractiveRegion::child_with(builder)` |
| `LayoutObserver::from_callback(callback)` | `LayoutObserver::from_shared_callback(callback)` |

统一约定：

- `new` 只接必要配置，不接 child；必要回调仍可作为配置，不强行给观察器或动作处理器添加无意义的空回调默认值。
- `child` 设置单槽，`child_with` 通过构建函数设置同一槽位；两者互相替换，后者保留现有独立 Fiber、hooks 和订阅生命周期，不改成父 Fiber 内执行。
- `children` 替换多槽列表；`push_child / extend_children` 明确追加。F2 的空 block 清空与数量检查不因命名迁移改变。
- `from_shared_callback` 明确接收共享回调句柄，不扩大线程约束。
- 保留 `Button::text/icon`、`SizedBox::width/height`、`Separator::horizontal/vertical`、`ThemeProvider::from_signal`、Flex 的 `with_options` 等有明确语义的便利构造。

### 7.3 属性与回调方法

下表使用当前类型名定位旧入口；实施时与 7.1 的类型改名配套。

| 当前 API | 建议 API | 语义约束 |
| --- | --- | --- |
| `Focus::handle` | `focus_handle` | 与普通控件一致 |
| `Focus::order` | `tab_order` | 明确排序用途 |
| `Focus::enabled` | `focusable` | 只表达焦点资格，不改成整个控件的 disabled |
| `SizedBox::color` | `background` | 绘制的是盒子背景 |
| `TextLabel::color` | `foreground` | 内容前景 |
| `Icon::color` | `foreground` | 图标前景 |
| `TextField::color` | `foreground` | 编辑文字前景，不扩大为所有内部元素颜色 |
| `EditableText::color` | `foreground` | 与 TextField 一致 |
| `IconButton::color_with` | `foreground_with` | 状态驱动的图标前景，不覆盖表面背景 |
| `Notification::detail_color` | `detail_foreground` | 明确是详情部分 |
| `EditableText/TextField::on_changed` | `on_change` | 只改名称，仍仅报告用户提交文本的变化 |
| `EditableText/TextField::on_submitted` | `on_submit` | 保留 Enter modifiers 与触发时机 |
| `CustomPaint::handler` | `ExternalSurface::on_draw` | 明确回调用途 |
| `CustomPaint::schedule` | `ExternalSurface::schedule_with` | 调度需求查询 provider，不是事件通知 |
| `CustomPaint::ime` | `ExternalSurface::ime_with` | IME 状态/几何 provider，与动态 resolver 一致 |

其余约定：

- 保留 `disabled`、`selected`、`read_only`、`focusable`，不把不同状态合成一个开关。
- 保留 `on_click`、`on_activate`、`on_enter`、`on_exit`、`on_escape`、`on_unmount`、`on_input`。按钮 on_click 与通用区域 on_activate 的区分遵循 ADR 0081，不机械合并。
- 保留 `max_height_with` 的动态求值含义；`*_with` 接收计算配置或内容的函数，与 `on_*` 事件回调区分。
- `on_vertical_navigation` 名称保留，参数从 bool 改为建议类型 `VerticalDirection::{Up, Down}`；保持 IME 优先、仅现有垂直导航场景触发，不顺带添加水平导航。
- 普通 UI 回调仍为 UI-local，外部绘制/provider 保留既有所有权与调用时机，不因名称变化机械修改 Rc/Arc、Send/Sync 或 `'static`。

### 7.4 查询方法

建议保留 `.property(value)` 配置 builder；**Widget 配置查询统一采用 `get_property()`**。这是为避免 Rust 不支持同名 getter/setter 重载而选定的建议风格，不再混用 `get_`、`_value` 与无前后缀。

| 当前 API | 建议 API |
| --- | --- |
| `Align::get_insets` | 保留 |
| `Padding::insets` | `get_insets` |
| `DecoratedBox::decoration` | `get_decoration` |
| `DecoratedBox::clip_behavior_value` | `get_clip_behavior` |
| `Row/Column/Flex::options` | `get_options` |
| `IconButton::label` | `get_label` |
| `SizedBox::requested_width/height` | `get_requested_width/height` |

Controller 与只读值类型不机械添加 get_；保留动作动词和裸属性查询，但必须准确表达状态：

- `ScrollController::offset()` 建议改为 `requested_offset()`，与其“尚待成功布局夹取”的请求语义一致。
- `ScrollMetrics::offset()` 保留，表示已经提交的实际位置。
- `ScrollController::metrics/jump_to/scroll_by/reveal_range`、`TextEditingController` 的状态查询与编辑动作，以及不可变配置值的属性访问器不因 Widget builder 风格而统一重命名。

### 7.5 迁移与验证边界

1. 先固定最终映射及公开行为，再迁移类型/模块导出、构造器、方法、应用调用、宏 fixtures 与文档示例；模块文件名随最终类型名协调，不长期保留两套同义入口或兼容别名。
2. 保留 U1 已落地的 child 协议，与 U4 的数值/具名配置协同；不能把重命名当作行为修复。foreground 不扩大颜色作用域，focusable 不变成对子树输入的总禁用，请求滚动位置不伪装成已提交位置。
3. 验证公共 API 编译、跨 crate/依赖重命名宏、手写与宏声明等价、child 替换/清空、焦点/输入回调顺序及相关 docs 示例。涉及生命周期的行为仍保留原有 native/IME/HMR 验收义务。

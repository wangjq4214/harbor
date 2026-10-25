# Harbor Widget API 设计复审

## 1. 结论与基线

**应继续收敛现有 API，而不是重新启动一轮笼统的 API v2。** 根描述、普通 UI 回调、可组合 Button 和显式布局组件已落地；原报告中把它们列为待实现的问题已删除。当前优先事项是补齐 child 协议、事件消费/提交契约，再处理宿主入口和剩余配置一致性。

- 复审源码：`b5c41bbc3d78989cd3f2f0e5b624ea2a013cc7d6`。
- 开始工作树干净；结束仅修改本文件，未改生产代码、测试、ADR/spec 或其他审查产物。
- `crates/harbor-widget/` 已跟踪文件的基线 SHA-256 清单摘要：`acc8898032e61fb76b365f4972b5d081da5605610986d7e770d79e5c1876f14f`。计算方式：`git ls-files crates/harbor-widget | xargs sha256sum | sha256sum`；不含本文及构建产物。
- 模式：API 审查与建议。只允许更新本文；构建缓存和临时运行探针位于 `target/`，不新增源码或持久化 map/ledger。
- 审查完成指本次生产 API 范围的源码审查完成，**不代表以下问题已修复，也不代表 GPU、原生 IME、焦点及真实 HMR 卸载验收完成**。

优先级：P1 为优先处理的契约/行为风险，P2 为接口收敛或低层入口改进。下文区分“已复现”“静态确认”“建议”和“待运行验证”，不把设计偏好当作已发生的产品故障。

## 2. 旧问题的处置

保留原编号，避免关联 ADR/spec 失去语义；已解决项只留状态摘要，不再保留过时的问题正文、目标草图和整轮实施计划。

| 原发现 | 当前核实结果 | 本文处置 |
| --- | --- | --- |
| F1：根/child/内容 slot 构建边界 | 宏根和 child 都经 `IntoChildView` 延迟构建，已有 View 可作根；状态内容使用独立 ContentSlot Fiber | 删除旧问题。原生 Host 工厂仍有一处入口差异，单列 F9，不能据此否定已完成的宏/runtime 工作 |
| F2：child 数量及替换协议 | Padding、Align、Button、SizedBox 等已正确使用单槽；Flex/Row/Column/Stack 已提供显式替换/追加；但三个组件遗漏迁移 | 删除旧 Padding/Align/叶子问题，重开为下文精确限定的 F2 |
| F3：普通 UI 回调线程约束和命名 | Button/IconButton、MouseRegion、Notification 等已允许 UI-local captures；编辑器已采用 `disabled`，按钮已转发 focus handle | 删除。保留 `on_click` / `on_activate` 的既定区分，不再建议改成统一 `enabled` 或重命名全部点击入口 |
| F4：Button 组合与文本测量职责 | `new/text/icon/child/content`、共享 ControlSurface、TextLabel/Icon 测量绘制已存在 | 删除旧问题；不要求再新增公开 surface 组件或复制交互状态机 |
| F5：显式布局树及局部配置 | SizedBox 已支持可空单槽、可选轴和只读尺寸访问；Insets、Alignment2D、FlexOptions 及 fallible final-node 宏支持已存在，search/rail 已迁移 | 删除全部“尚未实现”的描述与目标代码；保留既定显式树方向，不新增通用布局扩展链 |
| F6：构造器、导入和命名 | Padding/Align/SizedBox 已根导出；Focus/Shortcuts/Actions 的构造/附加入口仍不一致 | 缩小范围、降为 P2；prelude 是可选方案，不是缺陷本身 |
| F7：数值校验 | SizedBox/Padding/Align insets/Flex gap 已在组件边界返回 Result；其他入口仍混用忽略、归零、布局错误和未经检查的配置 | 保留剩余部分，不重复要求改造已迁移的布局入口 |
| F8：具名配置 | Insets 与定位专用 Alignment2D 已解决原布局参数问题；导航 bool、material 数组及 Notification 的定位类型仍有改进空间 | 删除已解决部分，保留具体剩余项 |

主要依据：`construction.rs`、`view.rs`、`fiber/reconcile.rs`、`widgets/{button,interactive_region,sized_box,padding,align,flex,row,column}.rs`、宏 `expand.rs`、`tests/{child_attachment,fallible_layout,composable_button,local_control_callbacks}.rs`；设计约束见 ADR 0079–0084、spec 0034–0038。

**文档状态与实现状态分开：** 部分 ADR/spec 仍保留 Proposed/Draft、spec-only 或“未实施”的历史措辞，不能用其状态字段代替当前源码判断。本文不擅自更新这些文件的验收状态；原 F1/F3/F4 所记录的原生 IME/焦点与真实 HMR generation teardown 证据缺口继续保留，历史测试成绩也不算本次重跑结果。

## 3. 仍成立及新发现的问题

以下源码路径除特别注明外均相对 `crates/harbor-widget/src/`。

### F2 / P1：三个 SingleChild 组件仍在追加，能力声明与实际行为冲突

**已复现，置信度高。**

- `widgets/focus_scope.rs:36–55`
- `widgets/custom_paint.rs:84–117`
- `widgets/portal.rs:22–41`

三者都声明 `Capability = SingleChild`，但 `.child(...)` 使用 Vec push，`with_children(...)` 使用 extend，既不替换，也不验证 incoming 数量。`with_static_children` 只负责静态能力约束，动态检查仍依赖组件实现，不能兜底此遗漏。

公共 API mounted 探针在三个组件上均得到：

| 操作 | 实际结果 | 单槽契约要求 |
| --- | --- | --- |
| `.child(a).child(b)` | 2 个 child | 只有 b |
| `view! { staged_with_a => {} }` | 仍有 1 个 child | 清空 |
| 手写 `.with_children(Children::from_iter([a,b]))` | `Ok`，挂载 2 个 child | `ChildConstructionError` |

因此静态两个 child 会被拒绝，动态两个却成功，宏与手写设置也不等价；原以为清除的子树及其状态/注册仍会存活。

**建议：** 按已选定单槽契约改用 `Option<View>` 和共享 `into_single`，多内容由显式 Stack 组合。若确实要把 Portal 等改成 MultiChild，必须先明确变更契约并迁移 setter/append/宏测试，不能只更换 marker 来保留错误追加。补覆盖全部内建组件的能力一致性测试，而非仅测试 Align、Button、SizedBox。

### F6 / P2：少数 wrapper 仍有两套构造方式，日常导入入口缺少一致规则

**保留的设计建议，置信度高。**

`widgets/focus.rs:31–65`、`widgets/shortcuts.rs:31–50`、`widgets/actions.rs:22–40` 仍分别采用 `empty/new(child)`、`handler/new(child, handler)`。它们实现了 WithChildren，却没有普通 `.child(...)` setter；手写代码不能像其他单槽 wrapper 那样直接组装和替换内容。Focus 的 `.handle(...)` 又与控件的 `.focus_handle(...)` 不同。

`lib.rs:47–61` 已导出新布局类型，不应再说 Padding/Align 必须深入模块。剩余例子是 Stack/TextLabel 位于子模块，Component/BuildCx/View 位于 `view` 模块，而扩展 trait 在根。

**建议：** 优先让构造只接必要配置，并补一致的 child 设置入口；统一焦点配置命名。再明确推荐导入规则，必要时增加受控 prelude。不要仅为减少 import 行数导出所有内部类型，也不要机械为全部组件生成 Props。组件名、构造器、属性、回调与查询方法的系统命名清单见[第 7 节](#7-widget-与方法命名收敛清单待实施建议)，不局限于这几个 wrapper。

### F7 / P1：剩余外观配置仍缺少可预测的验证边界

**静态确认；属于分域 API 收敛建议，不等同于所有入口都应改成 Result。**

| 当前入口 | 行为/问题 | 位置 |
| --- | --- | --- |
| `Icon::size` / `IconButton::icon_size` | 非法值保留旧值，调用看似成功 | `widgets/icon.rs:27–40`、`icon_button.rs:76–84` |
| `TextField::padding` | 负值/非有限值归零；与外部 Padding 的 Result 策略不同 | `widgets/text_field.rs:42–50` |
| `Separator::thickness` | 原样保存，直到 layout 才报错 | `widgets/separator.rs:46–51,77–91` |
| `ControlStyle`、`Theme::spacing` | 公开原始数值，被 region/surface/图文布局直接消费，没有统一验证入口 | `theme.rs:15–29,101–106`、`widgets/control_surface.rs:15–37`、`button.rs:94–105` |
| `NotificationStyle` / placement inset | style 声明尺寸约束但未验证；placement inset 也原样接受 | `widgets/notification.rs:6–24,40–68,145–160` |

同类逻辑尺寸在相邻组件中可能变成错误、静默改值或晚期布局失败；用户难以在配置边界定位问题。样式的公开字段不必一律私有，但消费边界应负责验证，不应把“普通输入结构体”误当成“已经验证的配置”。

**建议：** 先定义尺寸、颜色、材质上限等各自的合法域及错误策略，再让等价入口共用验证。沿用已建立的“非法 scalar 在边界处理，依赖父约束或有效值组合溢出在 layout 处理”的区分。Backdrop 的有文档性能 cap、ScrollController 的导航请求归一化有实际语义，不应为了表面统一全部删除或改成 panic。

### F8 / P2：仍有同类型位置参数和过宽的定位类型

**保留并补充的设计建议。**

- `widgets/editable_text.rs:93–98`、`text_field.rs:85–89`：`on_vertical_navigation(bool)` 的 true 表示 down；应用 `harbor-app/src/palette.rs:471` 必须继续传递这个隐含约定。建议 `VerticalDirection::{Up,Down}`。
- `widgets/backdrop_filter.rs:50–111`：tone `[offset,slope,quadratic,saturation]`、rim `[highlight,shade]` 对调后仍可合法编译，真实 appearance 调用也确实在传这些数组。建议具名局部配置及同一验证路径；GPU 上传仍可保留紧凑数组。
- `widgets/notification.rs:6–9,139–148,247–260`：placement 横纵和 close alignment 仍接收含 Stretch 的 `Alignment`，实际只调用 position，Stretch 等同 Start。已有 `PositionAlignment/Alignment2D` 可表达正确能力，无需另发明一套定位类型。

不再要求重做 Insets 或 Align，也不删除 Flex 真正使用的 Stretch。

### F9 / P2：原生 Host 根工厂仍拒绝已经构造好的 View

**新增，静态确认，置信度高。**

`winit/host.rs:237–243,271–277` 的 `build/build_with_output` 要求 `R: Component`；`View` 并未实现 Component。相反，`Runtime::set_root` 已接受 IntoChildView。

结果是 `Ok(view! { ... })` 或返回 View 的普通组合函数可以挂载 Runtime，却不能直接作为 Host 工厂结果，调用者仍需额外包装组件/闭包。F1 的宏与 runtime 修复已经成立，但最外层入口尚未贯通。

**建议：** 两个工厂入口统一接收 `IntoChildView + 'static`，保留工厂错误、bootstrap output 与挂载时机；增加普通组件、宏 View、已有 View 的公共编译契约。不借此重新设计根身份或 HMR。

### F10 / P1：KeyChord 的公开字段可绕过唯一有效的规范化路径

**新增，静态确认并验证值差异，置信度高。**

`widgets/shortcuts.rs:7–23` 允许直接构造/修改 key；只有 `KeyChord::new` 将 ASCII 字符转小写。`Shortcuts::bind` 原样存储，查找使用精确相等（同文件 `47–50,86–92`），而输入侧始终使用规范化构造（`runtime/event_router.rs:684–692`）。

例如结构体字面量 `KeyChord { key: Key::Character('A'), modifiers }` 与 `KeyChord::new(Key::Character('A'), modifiers)` 不相等；前者绑定的 uppercase key 不会匹配经过小写规范化的事件。构造形式改变了快捷键是否可用，而类型未表达这个不变量。

**建议：** 收紧字段并提供访问器，或在绑定/比较边界统一规范化。明确重复 chord 的覆盖/优先规则，保留现有 modifier 精确匹配及 keypad 区分。补结构体输入、大小写事件和重复绑定的公共路由测试。

### F11 / P1：事件“已消费”有两个互不一致的协议

**新增，公共 API 已复现，置信度高。**

`EventHandled` 描述是否消费事件，`PrimitiveLeaf::handle_event` 和 `CustomPaint::on_input` 也返回它；但 `runtime/event_router.rs:1007–1012` 丢弃 handler 返回值，capture/target/bubble 是否继续只看 `EventCtx::stop_propagation`。相关入口：`input/event_ctx.rs:5–11`、`view.rs:521–529`、`widgets/custom_paint.rs:9–13,175–180`。

探针：InteractiveRegion 内放一个 PrimitiveLeaf，leaf 对 Down/Up 返回 Handled、未显式 stop；鼠标点击后**父 region 仍激活 1 次**。因此外部实现照返回类型的字面契约使用，会丢失“子控件优先消费”的预期。

**建议：** 明确定义“处理过”和“停止传播”是否独立；如果 Handled 就是消费，应由 router 统一兑现；如果刻意独立，应改清晰的名称/文档及扩展契约，而不是要求使用者从内建实现猜出还要调用 stop。先用内建/外部控件、capture/target/bubble 与嵌套激活的 characterization 测试固定行为，不能机械把全部 Handled 自动 stop 而不检查边界事件等现有语义。

### F12 / P1：hover 重绘结果会短路当前事件的命令提交

**新增，公共 API 已复现，置信度高。**

`runtime/event_router.rs:395` 使用 `hover_needs_redraw || self.finish_event(arena, ctx)`。当 enter/leave 回调要求重绘时，左侧为 true，当前 Move 事件积累的 focus/capture/clipboard/external-input 等命令不会经过 finish_event。

探针：MouseRegion 的 on_enter 调用 invalidate_paint，内部 PrimitiveLeaf 在 Move 中 write_clipboard。首次进入的 Move 返回 `clipboard: None`，同一路径第二次 Move 才返回 `Some(Write("move"))`。丢失的是当前 Move 的命令，并非已单独提交的 enter/leave 回调命令。

**建议：** 无条件完成当前事件提交后再合并重绘标志。新增“hover 转换和目标事件均有副作用”的测试，保留原命令顺序和恰好一次提交；无需改变公开回调签名。

### F13 / P1：Host 的致命 GPU 错误分类缺少完整的生产回传接缝

**新增，静态确认接线缺口；实际设备故障表现待 GPU/native 验证。**

`winit/presenter.rs:537–545` 公开 OutOfMemory/DeviceLost 等 FrameError；应用 `src/shell.rs:977–998` 依赖 HostFrameOutcome::Fatal 决定退出。可是 `winit/gpu.rs:189–199` 创建设备后未接入 device-lost/uncaptured-error 通知，`presenter.rs:324–369` 的编码闭包直接返回 Ok，submit/present 也没有错误回传；现有生产分类主要覆盖 surface acquisition/recovery。

所以不能把公开 Fatal outcome 当成实际 GPU 失败的完整观测渠道；仅靠注入 FrameError 的策略测试不能证明真实设备错误会进入该出口。这里不声称已复现设备丢失、内存耗尽或每种驱动的具体表现。

**建议：** 由 SharedGpu 持有设备级错误状态及唤醒机制，在帧/宿主边界转换成明确结果。说明异步错误的报告时机及“提交/请求呈现”并不等于 GPU 执行完成；不承诺同步捕获所有错误。验证设备错误注入、多窗口共享设备及真实设备失败的策略。

### F14 / P2：公开 QuadRenderer::encode 缺少绘制顺序契约，默认使用存储槽顺序

**新增，静态确认，作为低层 API 改进建议；不表示 Runtime 当前绘制顺序有错。**

`renderer/quad.rs:630–668,682–706,998–1011` 的 update 保留/复用 GPU slot，encode 按整个 slot 区间绘制，不按 SceneItem.paint_order。Runtime 则在 `runtime/frame_encoder.rs:131–150,505–575` 按场景顺序查 slot 并分段编码，补上了关键排序协议。

外部调用看似自然的 `update(delta) → encode(pass)` 在插入、重排或槽位复用后可能产生错误遮挡；稳定存储槽不是稳定绘制顺序。已有 `tests/quad_renderer.rs:300–349,1618–1652` 的槽复用测试不能替代绘制顺序契约。

**建议：** 推荐 Runtime 的 paint-ordered 入口；将 slot/range 操作收为内部，或明确命名为 storage-order draw 并提供接受 ordered items 的完整公开路径。保留内部槽复用和批处理，不为此重写渲染器。

### F15 / P2：host_deadline 在不可绘制窗口下被静默忽略

**新增，静态确认行为；deadline 的承诺范围需要明确。**

`winit/host.rs:679–689` 接受 host_deadline，经 presenter 转交 scheduler；`winit/scheduler.rs:134–157` 的 `!drawable` 分支直接返回 Wait，不考虑这个 deadline。可绘制路径才仲裁 runtime/recovery/host deadline。

如果调用者把非渲染的宿主计时也交给该参数，零尺寸/最小化且无其他事件时就不会按期唤醒。当前应用还会自行仲裁通知等业务 deadline（`src/shell.rs:776–824`），所以不能据此断言它们现在都失效。

**建议：** 区分 render deadline 与 host/business deadline。不可绘制时只暂停 redraw/GPU 动画，仍保留未来的宿主 WaitUntil；或者明确将该参数限定/命名为渲染计时，并要求调用者自行仲裁业务计时。补零尺寸下有/无未来 deadline、恢复 drawable 的纯状态测试。

### F16 / P2：内部构造和订阅控制面仍暴露给普通使用者

**新增，公共 API 已复现，置信度高。**

- `view.rs:575–585` 的公开 `View::new` 依赖 crate-private AnyView，文档又称它用于 crate 内部类型。外部不能实现该 trait，却能传入已有内建 widget 和任意 children，绕过能力协议。探针用 Padding 加两个 child 成功构造/挂载，直到 layout 才出现 `MissingPlacement`。
- `signal.rs:187–208` 的公开 subscribe/unsubscribe 隐含依赖私有 RuntimeScope；在正常外部调用点，默认使用 RuntimeId(0)，而真实 Runtime 使用独立 ID（同文件 `20,49–55`，`runtime/mod.rs:99–101,274–279`）。探针在 runtime 外对 root_id 调用 subscribe 后写 Signal，组件构建次数仍为 1，没有得到调用者期待的订阅。

**建议：** 把只在内部上下文有效的入口收为 crate-private；公开组合用 Component/IntoChildView/View::leaf，公开追踪用 BuildCx::track 或 Store::watch。若必须支持外部控制，应提供带明确 runtime 所有权和清理责任的能力，而不是泄露裸 FiberId + 隐藏 TLS 前置条件。保留 Runtime::arena 等只读诊断能力，不把所有 introspection 一并删除。

## 4. 依赖有序的后续单元（仅建议，未实施）

| 单元 | 对应发现 / 前置 | 写入所有权与读取依赖 | 保留契约、验证及预期收益 |
| --- | --- | --- | --- |
| U1：补齐构造能力 | F2；无前置 | 三个遗漏 widget、共享 construction 测试；读全部 WithChildren 实现及实际调用 | 单槽替换/清空、静动态数量一致；mounted 验证卸载与注册释放，消除虚假的 SingleChild |
| U2：固定事件消费并保证提交 | F11、F12；先 characterization，再改 router | event_router、EventHandled/扩展文档、输入回归；读内建控件、PrimitiveLeaf、CustomPaint 与应用桥接 | 嵌套优先、capture、边界事件、命令顺序和一次提交；验证本文两个运行探针对应的正式回归场景 |
| U3：规范化及内部边界 | F10、F16；构造收口依赖 U1 | shortcuts、signal、View 构造及实际消费者；读事件查找、RuntimeScope、hooks 清理 | 大小写/modifier/keypad 行为、身份/订阅生命周期不变；编译契约与 mounted 路由测试，减少可编译的无效操作 |
| U4：命名、具名配置与一致入口 | F6、F7、F8；验证策略先于消费者迁移 | 各配置所属模块、wrapper 导出、相关文档及应用声明；读 theme、layout、material 消费路径 | 不新增万能 Style/DSL；非法值矩阵、等价布局/样式、父约束和业务默认值回归；减少参数含义与错误时机猜测 |
| U5：贯通宿主与 GPU 结果 | F9、F13、F15；各子项可独立推进 | winit host/gpu/presenter/scheduler、公共编译/状态测试；读 main/dialog、多窗口与 HMR 接缝 | Host 所有权、工厂错误、deadline 仲裁及 teardown 顺序不变；组件/View 工厂编译、故障注入及独立原生验收 |
| U6：明确 renderer 公开层次 | F14；无强制前置 | renderer 导出/入口、独立消费者和文档；读 Runtime frame encoder 与场景排序 | 内部复用/批处理保留；重排、删除再插入及半透明重叠的 GPU 验证，避免调用方承担隐含 slot 协议 |

公共契约改变需先固定迁移策略，不把本次文档更新视为实施授权。共享文件和互相依赖的迁移应串行或原子集成；不以“文件更少/LOC 更低”作为收益标准。

## 5. 覆盖、保留理由与限制

### 源码覆盖

以 Git 清单逐文件核对；先 CodeGraph 定位，再读取源码。共 81 个含生产实现的文件（含一个 WGSL），不把搜索命中或失败子审任务算作已完成检查。

| 范围 | 覆盖 |
| --- | --- |
| `src/widgets/` 的 32 个文件 | 全文检查，包括模块导出和 ControlSurface；逐一核对 child 能力与相关配置/交互实现 |
| `src/renderer/`、`src/winit/` 的 15 个文件 | 只读子审全文检查，主审复核保留发现的实现及 main/runtime 消费接缝；包含 shader、文件内测试 |
| 其他 34 个生产文件：顶层 12、layout 7、input 4、scene 3、fiber 5、runtime 3 | 全部生产段已检查；其中 30 个文件全文检查，4 个文件仅文件内测试为部分阅读，见下条 |
| `fiber/mod.rs`、`fiber/layout.rs`、`runtime/mod.rs`、`runtime/frame_encoder.rs` | 生产段分别完整覆盖 1–29、1–427、1–982、1–578；后续内嵌/引用测试没有全量审计，文件级状态为部分覆盖 |
| 独立测试模块与 `tests/` 的 27 个文件 | 已盘点；只做代表性契约阅读/定向执行，不宣称测试源码全部检查 |
| Cargo.toml、README、Lucide 文档/资源 | 检查特性边界、公开示例、资源范围与许可；没有新增资源体系 |

外部接缝定向检查：宏 expand/编译 fixtures，`harbor-app` 的 search、rail、palette、appearance；根应用 `src/shell.rs`、dialog/HMR/effects 的相关宿主路径；`docs/widget/catalog.md` 及相关 ADR/spec。外部文件并非全量审计。未检查全仓业务、第三方 wgpu/平台/HMR 库实现，也未将未读测试区域视为无问题证明。

### 应保留的边界

- 普通 Rust 表达式宏、单次求值/分支惰性/children-before-parent，以及显式布局树；不新增 JSX、属性 DSL、通用布局方法链或万能 Container。
- 独立内容 slot Fiber、parent-local key/type、直接 flex parent data 和原子布局提交；不承诺任意 wrapper 改写或跨父级移动保状态。
- 共享交互状态机、私有 ControlSurface、Button/IconButton 各自 theme role，以及 controller → editing core → decorated field 分层。
- Store/Dispatcher 的 FIFO 与 Host drain/wakeup 分工；没有自动 wake 是已说明的契约，不是本轮新缺陷。
- Host 持有 Window/Surface/Runtime，应用持有 EventLoop、业务、多窗口和退出策略；HMR 代际门控及销毁屏障有真实责任，不应当作无价值转发删除。
- 有界 Lucide 图标、借用 GPU 能力、原生资源析构顺序、应用 glass/命令策略边界。没有 UIA、虚拟列表或完整 shaping 系统是已说明的能力范围，不据此无限扩张本次 API backlog。

## 6. 本次检查与剩余验证

| 命令/检查 | 本次结果 |
| --- | --- |
| `cargo test -p harbor-widget --test child_attachment --test fallible_layout --test local_control_callbacks --test composable_button` | 24 项通过，0 失败 |
| `cargo test -p harbor-widget-macros` | 10 项单元测试、1 项依赖重命名、1 项 trybuild harness 通过；harness 内 28 个 pass/fail fixtures 全部符合预期；0 doctest |
| `cargo check -p harbor-widget --all-targets --all-features` | 通过；存在已有 unused import/variable 警告 |
| 三个 stdin 编译的公共 API 探针，产物 `target/ui-api-{review,event,internal}-probe.exe` | 确认 F2、F11、F12、F16 的上述行为，及 F10 的构造值差异；未新增/修改仓库测试源码 |
| `git diff --check`、源码清单摘要及最终 changed-path 检查 | 通过；仅修改 `UI_API_REVIEW.md`，crate 源码/文档/资源清单摘要与基线一致 |

宏测试产生一个增量缓存目录 access denied 的提示及 fixture 的 unused_parens 警告，但命令退出为 0，测试均通过。临时探针初次编译曾因错误 import/私有字段/缺少宏 Cargo 环境及 Windows native library 搜索路径失败；修正探针与链接环境后成功运行，这些失败不是产品测试失败。

本轮未运行 workspace 全套测试、GPU 像素/真实设备故障测试、原生交互、IME 或实际 HMR DLL 卸载验收。F13 的真实错误报告链、F14 的独立入口 GPU 遮挡证据及既有原生/HMR 义务仍待验证。**现有定向测试通过不能关闭本文已复现的遗漏，也不能把“API 实现已存在”升级为完整需求验收通过。**

## 7. Widget 与方法命名收敛清单（待实施建议）

本节记录复审后的命名讨论，归入 F6，并与 F2/F7/F8 协同。**以下是建议目标名，不是当前可用 API、最终公开签名或已实施结果。记录清单不等于授权本轮修改实现。** 不因名称不够整齐就重写组件职责；类型名只改职责不准确的，方法名重点统一设置、查询、动态配置、事件与追加的规则。

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
2. 与 U1 的 child 协议修复、U4 的数值/具名配置协同，但不能把重命名当作已修复行为。foreground 等名称变化不扩大颜色作用域；focusable 不变成对子树输入的总禁用；请求滚动位置不伪装成已提交位置。
3. 验证公共 API 编译、跨 crate/依赖重命名宏、手写与宏声明等价、child 替换/清空、焦点/输入回调顺序及相关 docs 示例。涉及生命周期的行为仍保留原有 native/IME/HMR 验收义务。
4. 本次仅记录清单并同步 F6/F8 用语，未实施改名，也未因追加本节重跑代码测试；第 6 节记录的是前一轮源码复审的执行结果。

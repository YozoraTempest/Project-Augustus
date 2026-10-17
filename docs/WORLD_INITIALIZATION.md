# 原版世界载入与时间推进

当前可以从本地 EU4 1.37.5 安装目录载入地图、国家和省份的部分开局状态。兼容模块处理文件与历史日期，将受支持的数据交给 Rust 核心；命令行和 Godot 从同一世界查询结果。

项目以终版及全部 DLC 机制为开发基线，默认玩家具备全部 DLC，不根据启动器开关选择机制。当前可以推进日历并执行税收月结，完整经济、战争、AI 和事件尚未接入，不能运行完整战役或加载 MT，也未支持原版存档导入。

已支持的 `has_dlc` 条件按这一固定基线求值，识别 22 个玩法资料片和六个原版脚本引用的旧内容包。国家开局条件、自治条件和事件编译共用名称识别；未知名称继续报告缺项或编译错误，不默认为已拥有。该判断不代表相应 DLC 的完整机制已经实现。

## 运行

安装 Rust 工具链，使用自己的游戏安装目录：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --date 1444.11.11 --country HAB --province 134
```

输出 JSON 包含载入报告，以及可选的国家、省份查询和对应文件来源。国家查询分别保留科技组与历史单位组（`unit_type`）。省份基础税收、生产、人力用千分位整数表示，例如 `6000` 表示 `6`。未知国家或省份、错误日期、无效引用和不匹配的地图尺寸会报错。加载器要求安装目录版本为 1.37.5；仅修改版本声明不能使其他版本兼容。

需要读取目录 Mod 时，可重复提供 `--mod`，并用 `--launcher-config "<EU4 用户目录>/dlc_load.json"` 按启动器的 `enabled_mods` 筛选；Godot 启动参数也接受这些选项及 `--user-directory`。启用列表中的每个 Mod 仍须提供其目录。默认使用安装目录内可用的全部 DLC，不读取启动器的 DLC 禁用列表。

省份查询的 `metrics.devastation` 使用千分位整数，`null` 表示当前数值未知。直接荒废度效果、查询和按比例生效的省份属性已接入；月度恢复与繁荣度尚未实现。开局脚本未能完整执行时，不会把初始化的零当作最终值。

国家查询的 `metrics.stability` 使用整数，范围为 −3 到 3，`null` 表示未知。已接入国家作用域下的字面量 `stability` 条件与 `add_stability` 效果，小数部分向零截断；正负属性分别读取 `positive_stability` 和 `negative_stability`，税收与人力查询从省份所属国家读取当前修正。1444.11.11 的无歧义历史使用启动前零值，随后执行已支持的启动内容。尚未验证的历史赋值、间接历史操作、其他开局日期或失败的启动执行保留为未知；未支持国家间稳定度比较、购买稳定度及相关成本系统。

`--country` 只选择查询对象。需要明确控制身份时使用 `--player HAB`，可以重复提供多个玩家国家；其余已载入国家标记为 AI。`--ai-only` 明确指定没有玩家国家，不能与 `--player` 同用。名单在启动效果执行前绑定，身份随存档保存；读档时使用相同的开局名单，恢复保存时的当前身份。不提供名单时身份未知，需要 `ai` 判断的启动内容会报告缺项。此功能提供条件查询所需的身份，尚未接入 AI 决策或原版事件界面。Godot 的国家下拉框目前也只用于查询。

Godot 界面使用 Godot 4.7.2：

```sh
cargo build -p augustus-godot --locked
godot --path godot
```

选择安装目录和开局日期，点击“载入开局”。地图显示当前已载入的领土颜色；国家下拉框可查看领土、首都、文化、宗教、政体、统治者、外交关系、已载入的科技等级与重商主义，点击地图可查看省份、历史记载建筑、商品基础价格、贸易节点与下游连接，以及永久历史修正的数值声明。右侧面板可以滚动；点击省份后自动显示省份详情。也可以在启动时提供目录：

```sh
godot --path godot -- --game-root "/path/to/Europa Universalis IV" --date 1444.11.11
```

未载入的值显示为“未载入”。界面列出未应用历史字段的数量和存在历史文件歧义的国家数；这些内容仍需实现或验证。重新载入失败时保留原来的世界。

## 日期、月结与存档

载入后默认暂停。可以点击“推进一天”“至下月”，或点击“运行”连续推进、“暂停”停止。“保存”“读取”使用 Godot 用户数据目录中的 `augustus-world.json`，不修改原版安装或原版存档。保存会替换这个项目存档；读取后保持暂停。

命令行可以推进日期，在中途保存并继续：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --country HAB --seed 8351375 --days 45 --save augustus-world.json
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --country HAB --load augustus-world.json --days 320
```

新 EU4 世界的随机种子通过 `--seed` 指定，默认是 `0`；恢复时以快照保存的随机状态为准。EU4 快照会保存种子、抽取计数、624 项状态数组和当前索引；旧版通用快照仍可读取。`simulation.eu4_random` 报告当前种子和计数。`--date` 始终指定开局日期，恢复时须与保存时一致。命令行拒绝覆盖已有文件。存档核对受支持内容、开局数据和所选月度系统，内容变化、错误存档或不匹配的世界会报错。默认开局推进 365 天到达 `1445.11.11`，跨过十二次月结。

已验证的范围覆盖原版随机状态的初始化、单值生成、1,000 值批量生成及按保存计数重建。具体游戏系统如何把随机值映射为区间结果，以及非固定种子的时钟输入，仍需原版对照；因此这不代表所有随机消费者已与 EU4 一致。

日历每年 365 天，二月固定 28 天。每月首日执行一次月结，开局当天不结算；同日任务先按登记顺序执行，再计算税收。标准税收系统按当前所有权汇总省份 `tax_after_autonomy` 年化属性，除以十二并转为千分位货币，向零截断。这个执行顺序与取整约定已通过自制样本验证，仍需原版运行对照。

国家查询的 `last_monthly_tax` 保存最近一次结算的已知部分、完整收入、实际入账、前后余额和缺项；启用生产执行器后，`last_monthly_production` 保存按国家汇总的年度值、月度值、逐省结果、国库前后余额和缺项。只有完整收入和原有余额都已确定才入账。税收缺项会使之后的余额保持未知，不能把旧余额或部分收入当作当前准确余额。原版开局的国库及部分税收来源尚未确定，因此当前原版推进会产生有缺项的记录；生产、贸易、维护、借贷和人力结算尚未全部接通。

Rust 原生系统可以实现 `MonthlySystem`，在开局前整体替换月度执行者；存档记录系统标识和版本，恢复后需要同一实现。示例见 `crates/augustus-core/tests/calendar_taxation.rs`。兼容输入与原生输入使用同一个核心和调度入口，当前接口尚不是稳定 Mod SDK。

每日业务也有同样的替换入口。实现 `DailySystem` 后，在开局前调用 `select_daily_system`；核心会在每个推进日调用一次 `tick`，再处理当天任务和月度执行器。存档只保存执行器标识，恢复后必须重新提供相同标识的实现；没有选择每日执行器时，现有 `advance` 行为不变。核心自带的 `DailyEventRules` 提供了翻译后事件候选的通用执行器：普通组先冻结资格再按声明顺序派发，加权组只选一个正权重候选，事件目录自己的 once 记录继续负责重复触发限制。它只接收已经翻译好的事件候选，不替兼容层猜测 EU4 的具体日常规则。

原生内容也可通过 `Taxation::with_country_annual_attribute` 指定国家级年化税收属性。EU4 加载器会将 `all_nations.global_tax_income` 用于已知拥有省份的国家；当前皇帝还会按 HRE 成员中已选 `free_city` 改革的国家数，计入 `free_cities_in_hre.global_tax_income`。无领土国家的特殊资格尚未接入，年税保留为未知。未知国家属性会阻止实际入账，已知省份小计仍保留；配置与账项参与保存和恢复校验。原版 HRE 状态变更后的缓存刷新时序尚未完整验证。

## 当前范围

载入固定地图的省份定义和位图，区分陆地、海域、湖泊、荒地与固定地图未使用的定义。国家定义提供标识和颜色；国家历史处理首都、主文化、宗教、政体和科技组。省份历史处理所有者、控制者、核心、文化、宗教、商品、基础税收、生产、人力、城市状态和帝国归属。

先处理无日期的基础字段，再按时间顺序处理不晚于所选日期的条目；同日条目保持文件中的顺序。历史派生字段缺失时保留未知状态。同一国家有多个历史文件时，报告所有候选文件，暂不选择其中一个；首都必须属于该国才能绑定核心关系。

未支持的历史字段记录次数和一个实际文件位置，不执行其效果。因此，当前显示的国家数量和领土信息不能作为完整原版开局已经一致的证明。尚未接入军队、完整经济结算、事件、继承和外交操作等游戏系统。当前入口只读取原版目录，Mod 组合仍使用独立的[文件检查工具](EU4_CONTENT.md)。

## 人物与外交

国家历史中的君主、继承人及配偶会载入核心，保留姓名、王朝、出生日期、历史死亡日期、三项能力、宣称强度、摄政和来源国家等受支持字段。缺少的能力值保持未知；已超过声明死亡日期或尚未出生的条目不作为当前角色，报告保留其来源。嵌套的军事领袖、人物性格和未支持字段仍会报告。

外交历史载入同盟、王室联姻、保障、警告、贸易联盟成员、属国关系，以及两个皇帝职位。属国类型须在本地定义中存在。无向关系统一两国顺序；保障、警告、贸易联盟和属国关系保留方向。重复关系合并为一个核心状态，同时保留各条来源。核心拒绝自我关系、无效国家引用、多个宗主或循环属国关系。兼容事件在国家作用域中支持 `set_emperor = yes`，直接更新帝国皇帝职位；正常选举流程仍未接入。

当前日期选择采用包含开始日、不包含结束日的区间；各文件中的皇帝变更统一按日期排序。这些输入规则通过自制用例和本地原版文件载入检查，尚未完成与原版运行状态的逐项对照。

使用 `--country` 查询该国人物和外交，使用 `--diplomacy` 查询全局关系及皇帝职位：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --country FRA --diplomacy
```

人物与外交已纳入核心存档及恢复校验，已有建造演示存档仍可读取。历史中的未来死亡或关系结束日期仅用于选择开局，不会安排战役中的后续变化；实际继承、关系变化和相关结算尚未实现。

## 建筑、商品与贸易网络

建筑目录读取明确声明的费用与工期；费用采用千分位整数，工期保留月数，不换算成固定天数。省份历史中以建筑标识为字段的 `yes` / `no` 条目，按开局历史顺序增删建筑。商品目录与价格目录合并为核心定义，保留兼容层观察到的运行时索引、基础价格和声明的金矿类别。运行时索引从 1 开始，按选定内容中 `common/tradegoods` 的定义顺序分配；核心把它当作可选的外部标识，不用它推导动态价格。缺少的费用、工期、价格或类别保持未知，不补默认值。

贸易节点保留位置省份、成员省份、下游连接，以及声明的内陆和终点标记。核心检查引用、重复归属、自我连接和循环网络。输入中的重复定义会报错，当前入口不猜测其覆盖顺序。省份没有历史文件时，建筑查询返回 `null`；有历史文件但没有受支持的现存建筑条目时返回空列表。这份列表表示历史输入结果，尚未执行建筑保留条件。

使用 `--economy` 查询建筑与商品目录、完整贸易网络；报告的 `economy` 字段给出数量、定义来源、建筑历史的最终声明位置和未应用字段。原有省份 JSON 字段继续保留，新增 `buildings`、`trade_node`、`trade_node_outgoing` 和 `trade_good_definition`。查询单个省份时还会给出 `province_trade_good_base_price_milli`，它只表示 `common/prices` 中声明的商品基础价格，不代表动态市场价格或已经完成的贸易结算。Rust 调用方可以使用 `VanillaWorld::calculate_vanilla_province_production_base_value`，在明确提供货物数量及修正项时复用该基础价格；没有已知商品价格时返回未知，不补默认值。

建筑定义会保留标准建筑与要塞的角色及要塞等级。要塞不参与普通省份属性的建筑修正汇总；防御区域、维护和战斗路径尚未接通。

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --province 134 --economy
```

这些数据与人物、外交一样进入同一个核心及存档，原有建造演示存档仍可读取。部分建筑与商品的省份数值修正已进入下述属性计算；自动生成生产上游、建造与保留条件、回调和贸易收入仍未全部接入，但兼容层提供明确输入时，月度生产执行器会把逐省结果写入国家国库并保留结算记录。动态价格可以由外部状态按运行时索引提供，兼容层会在生产值阶段重建有效价格；绘制路线的路径和控制点也未接入。核心已经提供贸易传播金额、保留比例、路线权重归一化、候选排序和价值下推的独立算子，兼容层仍需提供省份产出、国家权力、路线和政策等上游状态后才能组成完整贸易结算。未处理的定义字段会记录在报告中。显示的基础价格不能当作实际商品收入，贸易连接也不能当作贸易结算已经完成。

位图、脚本和生成报告均来自本地安装，不随项目分发。需要完整资源的 DLC 档案读取尚未接入；当前地图与所列开局字段从安装目录的普通文件读取。

## 政体与改革

国家等级按开局历史、改革等级、属国上限和皇帝身份初始化，并保存在共享政府状态；受支持的国家脚本可用 `government_rank = N` 查询、用 `set_government_rank = N` 修改。政体查询也包含已选改革及合法性状态。基础改革从政体定义读取，历史中的改革增删与合法性按日期处理；同层级的后选改革替换前项。可确定的 DLC 和常量条件会应用，未知条件、重复定义保留报告。只有已确认使用合法性的政体才载入该指标；普通君主制的 `1444.11.11` 历史初值为 100，明确历史值优先，其他日期不补默认值。启动事件和外交初始化对合法性的后续改动尚未执行，因此它不等于完整开局存档中的最终值。使用贤能值、游牧团结或奉献值的政体不会附加合法性效果；贤能值与游牧团结尚未接入，共和国传统和奉献值的开局数值及属性效果仍未确定。

国家查询新增 `government` 和 `declared_government_effects`。合法性按静态定义缩放数值声明，改革的直接税收、人力修正进入省份计算；宽容度、叛乱与外交等声明尚需对应系统消费。条件修正和改革触发效果尚未执行。原生内容也使用可整项替换并保存的 `Governments`；查询随当前改革、指标和所有者变化，不保存派生值缓存。

国家条件支持 `has_government_mechanic = <标识>`。兼容模块读取 `common/government_mechanics` 的定义和改革中的 `government_abilities`，应用可确定的常量及 DLC 条件，将已挂载的机制翻译成核心政体能力；查询可用于受支持的事件、阶层和理念条件，并随改革增删、所有者查询及存档恢复读取当前状态。多个改革可共同提供同一机制。需要国家运行状态的 `available` 条件、未知改革条件、重复定义及缺失引用保留为未知；尚未实现机制点数、互动、增长和动态可用性的刷新。

已接入部分 `change_government` 转换：同类型仍执行旧状态回调并保留改革与指标；跨类型先执行回调，再采用目标基础改革和活动指标，清除普通改革。当前支持分类明确的君主制、共和国、神权、部落君主制和原住民基础政体，游牧、贤能及其他未核对分类保留报告。共和国传统和奉献值在转换时初始化为 50，停用后可查询保留值；它们的历史开局初值和属性缩放仍未知，会影响依赖这些属性的初始化结果。阶层重建及其他跨系统生命周期尚未接入，完整原版启动仍可能被回调中的未支持脚本阻塞。自制用例见[政体转换场景](../examples/government-transitions/README.md)。

启动效果、其调用的事件和选项已支持字面小数 `add_legitimacy`；`legitimacy` 阈值条件读取同一份国家状态，也可用于受支持的理念与阶层选择条件。每次增减限制在 0 到 100，不适用该机制的国家忽略效果。启动执行未完成时，放弃中间写入，并将存活国家的活动指标值保留为未知；查询不会把开局前的值当作最终结果。正统性的自然增长、其他政体指标和历史文件中的 `add_legitimacy` 尚未执行。

启动和事件中的 `add_country_modifier` 已接受字面本地化键 `desc`，将其保留为效果的期限说明；真实期限和属性贡献仍走已有修正系统。说明不会作为附着状态写入世界。消息参数、动态说明和本地化文本展示仍未实现。

启动效果及所调用事件中的 `tooltip = { ... }` 使用核心 `Preview` 保留受支持的内部计划，执行时不应用内部效果。预览里的事件引用仍参与内容登记与检查，但不会触发事件或安排任务；未知效果和无效引用继续阻止启动。自动效果描述与本地化渲染尚未接入，使用与原生写法见[事件演示](EVENT_DEMO.md)。

`custom_tooltip` 的字面本地化键也可用于启动效果和事件，翻译为共享的 `Description`。它不改变模拟状态；隐藏容器中的提示保留隐藏标记。非法键仍会报告来源并阻止启动，具体写法和查询范围见事件演示。

## 阶层与特权

国家查询的 `estates` 列出阶层的可用状态、忠诚度、影响力、土地份额和已选特权；`declared_estate_effects` 给出已知属性声明及缺项。阶层可用条件复用国家条件解释器，支持部分政体、文化、宗教、DLC、历史标记及阶层依赖，并展开已接入的带参数条件脚本。未知条件和循环依赖保持未知。直接历史中的特权增删按日期处理，未执行的条件与作用域不会被当作已经应用。

阶层的三档属性、土地效果及特权的直接收益、代价进入共享核心。税收和人力公式按当前所有者查询这些效果；状态改变后重新计算。特权中受支持的条件修正使用世界的编译目录，读取当前政体指标、国家点数和任务状态；未知值继续阻止相关属性得到完整结果。原生内容可以提供完整的 `Estates`、内容定义的忠诚度阈值和影响力档位，通过 `set_estates` 整项替换，状态随存档保存。自制完整样本包含税收月结与保存恢复，见 `crates/augustus-core/tests/estates.rs`。

开局土地分配已接入共享核心的 `EstateLandRules`。兼容模块从基础与条件影响力、已载入的政体和历史修正生成分配权重，读取王冠权重与最低保留份额，再按声明顺序处理分配和舍入。直接历史中的 `change_estate_land_share`、`add_estate_loyalty` 按日期与条目顺序应用，生成的数值随项目存档保存，恢复时不重复执行。报告的 `initial_land_countries`、`initial_loyalty_countries` 统计已生成国家，`unresolved_initialization` 说明缺项；未完成的直接历史操作保留来源。

相关输入完整时，忠诚度从 50 开始，受支持的负向变化最低到零。`on_startup` 的效果直接写在入口内，以各存活国家为初始作用域。兼容入口已支持忠诚度与土地变化、特权授予和撤销、条件分支、国家作用域、国家迭代及无参数效果脚本。授予和撤销回调先执行，再更新特权列表；`has_estate_privilege` 查询当时的核心状态。数值变化与特权操作使用核心 `Action`，原生内容可以直接提交相同指令。

启动效果完整执行后才保留结果；遇到未知指令或条件时，放弃本段中间改动，将最终忠诚度、土地份额与特权列表保留为未知，并记录位置和原因。报告新增 `applied_startup_effects`、`startup_effect_sources`、`pending_startup_effects` 和 `startup_unresolved`。已完成的授予检查与回调按国家记录，随存档保存；恢复后不会重复执行，也不会替其他国家消除缺项。

启动事件从 `common/scripted_triggers` 和 `common/scripted_effects` 登记宏，使用已有编译器展开无参数调用；宏中的间接事件也进入同一目录。只读取直接 `.txt` 子文件，按原文件名字节顺序保留首个同名定义，忽略的声明位置见 `startup_script_ignored_definitions`。未调用模板不会因尚未支持的值形状阻止登记，实际绑定时仍检查并报告原位置；参数替换、循环和不支持的调用会阻止本次启动。自制输入及原生计划见 [脚本宏演示](../examples/scripted-events/README.md)。

启动处理会从 `missions/*.txt` 索引任务标识，支持国家作用域的 `complete_mission` 和 `mission_completed`。固定 `1444.11.11` 新局在启动效果执行前使用空完成记录；其他日期保持未知。登记、条件查询、选项和延迟事件共用核心状态，重复登记不执行奖励。启动失败时，完成记录与事件及其他候选写入一起撤销；成功结果随项目存档保存。任务系列的列、行和声明顺序由兼容层译成核心选择条件，开局先初始化可用任务；`has_mission` 查询当前选择，`swap_non_generic_missions` 显式刷新。启动失败时保留任务定义，可用状态和完成记录保持未知；加载报告通过 `missions` 给出未解决条件。任务完成条件、领取奖励和任务界面尚未接入。自制示例与原生接口见 [事件演示](EVENT_DEMO.md)。

固定 `1444.11.11` 新局的启动候选也会建立空变量空间，受支持的字面数值赋值、加减乘除和条件查询读写共享核心。空启动入口可以确定未赋值名称为零；启动未完成时，变量写入与其他候选状态一起撤销，最终变量保持未知。其他日期的变量初始状态尚未生成。数值和已知范围随项目存档恢复，详细语法与原生接口见事件演示。

国家启动效果、事件和选项的修正添加、移除及查询共用已有修正定义与核心附着记录。到期状态同时影响事件条件和属性贡献，保存恢复保留期限。历史中尚未确定的有限期限继续报告未知；启动失败会撤销候选修改，并将存活国家的最终修正附着状态标记为未知。国家和省份查询的 `modifier_attachments_known` 表示附着列表是否完整确定；`modifiers` 和 `declared_modifier_totals_micros` 保留已记录的声明，属性查询另行给出最终数值和缺项。定义中的未知属性在实际附着时影响计算，移除后不再影响。当前国家效果读取已载入的 `common/event_modifiers` 定义，省份期限、其他定义来源及完整历史期限生成仍待核对。

原版启动事件、历史中的非空特权回调、特权自身的非零土地附加、省份授予回调及重复授予等行为仍待接入或核对。未验证的上界、土地越界和未知作用域继续保留未知。正式原版开局的最终土地、忠诚度与特权列表尚未确定；分配权重也不能当作当前影响力。核心已经接入按国家注册顺序分摊的每日忠诚度更新和带符号衰减公式，但只有在忠诚度目标已经由内容或外部状态提供时才写入；原版目标的全部来源、界面中的阶层操作和议程仍未完整接入。

## 科技、国家资源与历史修正

科技组、三条科技表和 `common/units` 模板从本地定义读取，科技等级按表内声明顺序从 0 编号。每级的 `enable` 单位模板名称也按声明顺序载入，并可从 `--technology` 查询结果查看。单位模板可用 `--unit <文件名>` 按文件主名查询，其类别和声明字段按原顺序保留。仅在 `1444.11.11`，使用国家科技组明确声明的 `start_level` 设置行政、外交和军事等级；没有科技组或选择其他日期时，等级保持未知，不根据年份猜测。单位解锁条件、可用单位选择、升级费用、累积效果和研究过程尚未执行。

国家历史中明确声明的重商主义按日期载入，使用千分位整数，`25000` 表示 `25`。国库和可用人力保留未知状态，尚未计算其开局值。国家查询新增 `technology`、`resources`、`modifiers` 和 `declared_modifier_totals_micros`，原有字段继续保留。

事件修正目录保存可支持的数值与布尔声明。国家与省份历史中直接写出的添加和移除条目按日期处理；只有明确声明 `duration = -1` 的修正作为永久项载入。有限期限或缺少期限的条目记录在 `report.setup.pending_timed_modifiers`，不作为当前已确定的修正。嵌套条件、统治者修正、触发修正，以及静态修正的全部适用条件尚未执行；受支持的适用条件见下述属性计算。

省份查询包含 `modifiers` 与 `declared_modifier_totals_micros`。后者只对已载入历史修正中的同名数值声明求和，使用百万分位整数，`12500` 表示 `0.0125`，不在这个字段中处理作用范围、基础值、乘法、条件、布尔优先级或其他来源。界面将这些数值标为“声明值”；下述属性计算另行组合受支持的来源，不能将声明值直接视为最终游戏属性。

同一修正或科技等级重复声明数值或布尔属性时，报告在 `report.setup.ambiguous_properties` 中保留全部位置，该属性暂不取值。未支持字段保留数量和来源；定义标识重复或引用无效时报错。科技、资源和修正状态进入同一核心及存档，原有建造演示存档仍可读取。

使用 `--technology` 查询 `administrative`、`diplomatic` 或 `military` 的等级表，使用 `--modifier` 查询一个事件修正定义及来源：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --country VEN --technology administrative
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --unit sample_light_ship
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --province 224 --modifier guadiana_estuary_modifier
```

## 部分属性计算

省份查询的 `attributes` 包含 `tax_basis` 和 `manpower_contribution`；Godot 显示“已接入部分，自治前”，并列出参与计算的来源和缺项。税收基数从基础税收开始，人力贡献按本地 `NMilitary.BASE_MP_TO_MANPOWER` 的明确声明换算为人数。月结使用另一个自治后属性 `tax_after_autonomy`；完整国家人力上限及可用人力结算尚未建立。

当前组合城市状态、明确的占领状态、历史记载建筑、商品的 `province` 块、永久省份与所有者历史修正，以及文化待遇和宗教定义的直接效果。税收接入固定增量 `tax_income`、省份 `local_tax_modifier` 与所有者 `global_tax_modifier`；人力接入省份 `local_manpower_modifier` 与所有者 `global_manpower_modifier`。其他静态修正仍只是目录声明，未根据其名称自动生效。建筑保留条件尚未执行，参与计算的建筑仍是历史载入结果。

文化组从本地定义读取，接受文化的增删按所选历史日期处理，文化联盟按已接入的国家等级规则初始化。主文化、接受文化及联盟接受的文化不应用基础文化惩罚；同组和异组的未接受文化分别选择 `same_culture_group`、`non_accepted_culture` 中的数值。宗教只自动应用当前国教的 `country` 块和省份宗教的 `province` 块中已支持的属性；兼容入口不将次要宗教、转换回调或宽容度声明直接当成税收效果。`report.attributes.religion_definitions` 给出已确定的宗教定义数量；定义和属性重复时保留来源与缺项。

每条计算返回基础值、各阶段的固定增量、比例之和、输出与贡献来源。数值均使用百万分位整数；`known_subtotal_micros` 是已接入部分的结果，`unresolved` 列出缺项。只要存在缺项，`value_micros` 就是 `null`。基础值未知时，已知部分结果也为 `null`。国家查询的 `province_attribute_totals` 按当前所有权汇总省份贡献，同时保留缺项；尚有省份归属未确定时，只给出已知领土的小计，完整总量保持未知，明确无主的省份不会造成这一缺项。它不包含国家固定人力等尚未接入的来源。

完整自治与州规则、政体对文化待遇的影响、宽容度与宗教机制、理念、阶层、科技效果及其他来源还需实现和核对。当前结果不能作为完整原版税收、人力上限或收入。原有 `resources.manpower_milli` 表示可用人力，继续保持未知，不用省份贡献代替。

## 地区与自治

省份查询的 `administration` 提供地区、所有者的州或领地状态、所有者核心类型、首都关系、历史自治追加、规则最低自治及有效自治。自治使用百万分位比例，`250000` 表示 25%；这与原版文件中以百分数书写的 `25` 不同。非陆地省份目前没有行政记录。国家查询的 `administrative_areas` 只列已确定的状态，缺少条目表示未知。

地区来自 `default.map` 指定的 `area` 文件。支持省份列表，以及在列表前声明的三分量 `color`。同一地区重复列出省份时合并归属，在 `report.administration.repeated_province_memberships` 中记录重复次数；跨地区重复归属时报错。空地区标识保留并列入 `empty_areas`。地图没有声明地区文件时，原有入口仍能载入，陆地省份的缺失归属列入 `missing_land_areas`。

大陆成员关系读取 `default.map` 指定的 `continent` 文件，进入共享 `Administration.continents`；重复省份编号合并，跨大陆重复成员和无效引用会报错。`report.administration.continents` 统计载入的分组，包括已声明的空组。`island_check_provinces` 保留为未执行的连通性声明，不作为大陆；空的随机新世界预留组也不代表已支持随机地图。事件与受支持的开局条件可直接查询这些地理状态，详见 [作用域条件演示](../examples/scoped-conditions/README.md)。

在固定的 `1444.11.11` 开局，兼容入口为各国实际拥有省份的每个地区建立州，历史 `add_core` 载入为完整核心。地区关系按国家分别保存；其他国家的核心宣称不会替该国建立州。首都引用无效时仍按所有权建立州，不另选首都。报告的 `initial_states` 和 `full_core_types` 给出这两项数量。选择其他日期时，这两项生成规则暂不应用：只确定已绑定首都所在的地区为州，核心类型保留为 `unspecified`。

固定 `1444.11.11` 开局已接入原始自治度生成，使用所选日期的文化、宗教、核心及国家等级输入，再逐条执行省份历史和国家历史中的自治操作，每一步限制在 0%—100%。国家等级读取直接历史、政体改革的固定等级、属国类型的继承上限及皇帝职位；这些输入目前用于自治初始化，尚不构成完整政体系统。

国家历史支持 `every_owned_province`、具名地区作用域及受支持的 `if` / `limit`。具名地区包含别国与无主省份；拥有省份迭代器只选择本国省份。条件支持 `always`、固定终版的玩法 DLC 名称、`AND`、`OR`、单条件 `NOT`，省份条件另支持 `area` 和 `region`。地理分组读取 `default.map` 指定的 `region` 与 `superregion` 文件，进入共享行政状态。季风及 `restrict_charter` 等附加声明只记录在 `unapplied_geography_fields`，尚未执行相应机制。未知条件、作用域、歧义定义和尚未接入的嵌套历史不会被当成已完成操作。

报告的 `initialized_autonomy`、`unknown_initial_autonomy` 给出已生成与未知的省份，`applied_country_autonomy` 保留已执行操作的来源，`pending_country_autonomy` 和 `pending_province_initialization` 列出待处理输入。已生成时，`history_autonomy_delta_micros` 为零，追加量已进入原始自治度；生成未知或选择其他日期时，该字段继续保留直接历史追加量，不能作为最终值。首都的原始值也可非零，生效值另受首都上限约束。

规则最低自治从已载入的静态定义读取，首都上限使用 Lua 求值后的 `NEconomy.CAPITAL_MAX_AUTONOMY`。完整核心所在州可以查询规则最低自治，原始自治度未知时仍不能确定非首都省份的有效自治。其他最低自治修正和贸易公司规则仍有缺项。Godot 将这些字段标为已接入规则的结果。

税收与人力计算按当前所有者的行政状态选择 `core`、`colonial_core`、`non_core`、`territory_core` 或 `territory_non_core`。税收把行政、文化、宗教、建筑、商品及国家来源的税效率直接相加，不另加 100% 基础倍率；再将效率与 `1 − 有效自治` 相乘，并在千分精度截断后乘基础税。HAB 的 17 个省份在四份原版采集中与 442 条税收调用逐位吻合；其他国家、修正组合及未接入来源仍有缺项。人力继续使用比例修正通道。所有权、核心或州与领地关系改变后，查询和月结都会重新选择来源。分类未知、所选定义缺失或属性有歧义时保留缺项；不适用的分类不会影响当前结果。原生内容通过 `AttributeTerm::Administration` 提供自己的分类与静态定义映射，使用同一计算和存档接口；示例见 `crates/augustus-core/tests/administration.rs`。

国家理念读取 `common/ideas` 中的免费理念组，按文件和声明顺序检查候选。条件支持国家标识、文化与宗教及其分组、科技组、政体、改革、君主王朝、直接历史标记，以及 AND、OR、NOT。还支持特殊国家身份、政体属性，以及 `capital_scope` 下的 `region` / `superregion`。首都条件使用已绑定的核心首都；无效首都、未知身份和未支持的作用域保留缺项。较早候选的条件未知时不会直接采用后面的通用组。`report.ideas` 给出定义来源、身份来源、未支持字段和各国的未确定原因。

固定 `1444.11.11` 开局中的历史国家初始化为非自建附庸国、非新组建的联邦国家；联邦成员关系不改变这一身份。其他日期、歧义历史或未执行的创建操作不补此初值。当前殖民国家身份取决于实际属国关系及属国类型继承后的 `has_colonial_parent`；历史中的母国标识不代替当前关系。革命目标读取直接历史，革命政体与其他政体属性读取当前选定改革的 `revolutionary` 和 `custom_attributes`。重复声明、未知条件和相互冲突的选定改革保留未知。这些条件的载入不包含创建国家、联邦整合、革命传播和政体转换玩法。

`1444.11.11` 的选定国家理念以零解锁初始化，国家传统立即提供直接属性；其他历史日期的进度保持未知。税收和人力公式查询当前所有者的理念效果。核心 `Ideas` 支持多个理念组、内容定义的项目数量、顺序解锁及完成奖励，原生样本见 [native_ideas.json](../examples/world/native_ideas.json)。原生内容可用 `CountryPolitics.statuses` 提供显式国家身份，用 `GovernmentReform.features` 定义政体能力；身份缺失或值为 `null` 表示未知，政体能力在改革选择及定义完整时未声明表示不具备。通过 `set_ideas` 整项替换，状态和定义随存档保存；重复项目、越界进度和无效国家引用会被拒绝。重复直接属性保留缺项，未执行的解锁脚本影响其生效阶段的完整性。尚未实现花费点数解锁、理念切换及付费理念组的玩法。

新增 `tax_after_autonomy` 与 `manpower_after_autonomy`。税收效率与自治倍率在同一乘积中合成，乘积先截断到千分，再乘基础税并截断货币结果；例如 `0.670 × 0.941` 得 `0.630`，对应原版逐省输出。`tax_basis` 保留自治前税额，`manpower_after_autonomy` 仍将自治作为后续比例阶段。自治输入未知时，`tax_after_autonomy.known_subtotal_micros` 为 `null`；缺失的其他来源仍使最终 `value_micros` 保持为 `null`。

原生内容直接提供 `Administration`，可以设置地区、区域和大区分组，各国的州与领地关系、完整或领地核心、原始自治度和各类最低自治规则；查询使用当前所有者、首都及行政状态，支持整项替换。它与兼容内容使用同一核心、计算阶段和存档。行政状态恢复时检查地理分组的成员引用与跨组重复、国家和省份引用，以及比例范围；没有该字段的已有建造演示存档仍可恢复。

报告的 `attributes` 部分给出定义数量、来源、未映射属性，以及重复定义或属性的全部位置。静态修正同名定义暂不选择，其中的数值不会进入已确定来源；涉及重复项的计算保留缺项。定义输入在一次 Lua 5.1 求值中依次执行有效的 `common/defines.lua`、`common/defines` 直接子目录中的普通 Lua 文件，以及存在时的 `difficulty_normal.lua`；其他难度文件不会执行。已实现的系统共享这批结果，支持表达式和后续覆盖。数值仍按各系统的精度与范围校验，错误类型不会转换成零。普通直接声明保留字段位置，动态计算只报告最后改变有效值的文件级来源。

核心的 `Attributes` 可以整项替换，原生输入也使用同一个计算器。普通通道计算 `(输入 + 固定增量之和) × (1 + 比例修正之和)`；`ProductOfSums` 可将多个来源组相加、按顺序相乘，并按指定精度截断，供 EU4 直接税效率使用。运算溢出时报错。当前基础字段、所有权、建筑、商品和附加历史修正均在查询时读取，不保存派生值缓存；城市与占领来源的选择来自本次开局历史，尚未接入运行中的状态变化。自制 [原生属性样本](../examples/world/native_attributes.json) 与原版写法样本得到相同核心状态；这份样本用于贡献者测试，尚不是稳定的 Mod SDK。

共享核心的 `Affiliations` 保存实体的文化与宗教归属、分组和接受政策，可整项替换；`Attributes` 根据当前所有者及归属选择来源，修改这些状态后无需重建公式。原生内容通过同一接口提供状态和效果，样本见 [native_affiliations.json](../examples/world/native_affiliations.json)。接受政策与归属随核心存档保存，未知输入继续保持未知；这尚不包含战役中的文化转换、宗教转换或国家等级变化系统。

宗教学派定义和国家历史选择进入同一 `Affiliations` 的 `religious_school` 维度。报告的 `religious_schools` 给出定义、历史来源，以及已选择、明确未选择和未知国家的数量。历史声明按日期及文件顺序处理；同宗教组的变更保留选择，跨组变更清除。原生输入用 `absent_members` 表示明确没有归属，缺少成员或成员为 `null` 仍表示未知。查询与跨输入读档见 [宗教学派演示](../examples/religious-schools/README.md)。

学派块中的直接数值属性已载入共享 `Attributes`，税收和人力公式读取当前所有者所选学派的 `global_tax_modifier` 与 `global_manpower_modifier`。明确无学派贡献零，未知选择或相关属性有歧义时保留缺项；其他属性保留声明，不代表对应玩法已经运行。`report.attributes.religious_school_definitions` 给出已确定定义数量。开局与事件中的 `set_religious_school` 修改同一归属，保存和查询方法见 [学派变更演示](../examples/school-effects/README.md)。学者邀请、回调和运行时宗教转换尚未接入，数值计算尚未完成原版运行核对；历史初值也不代表未完成启动动作后的最终值。

国家历史、开局效果和事件共用核心 `Markers`。历史中的直接设置与清除按既有日期顺序处理；阶层和免费理念的初始条件从同一状态读取。国家查询的 `flags` 给出已确定的名称值，`flag_namespace_known` 表示其余名称是否也已确定。缺少或有歧义的历史，以及尚未执行的嵌套修改，保留未知；整体开局失败时撤销候选写入，将存活国家的最终标记空间标为未知。事件中的设置、清除和查询见 [事件演示](EVENT_DEMO.md)。标记变化后重新选择阶层或理念的动态系统尚未实现。

## 验证

当前君主的 `adm`、`dip`、`mil` 条件与 `change_adm`、`change_dip`、`change_mil` 效果已经接入。条件实时读取人物状态；变更范围从字面 `MONARCH_MIN_SKILL`、`MONARCH_MAX_SKILL` 读取，并在 `setup.ruler_skill_bounds` 中报告。历史能力在载入时保持原值，执行变更时才按范围钳制。缺少任一范围定义时，变更规则保持未知。人物生成、继位和完整开局仍需其他系统。用法见 [当前君主能力](../examples/country-ruler/README.md)。

国家历史中的字面 `national_focus = ADM/DIP/MIL` 已进入共享人物状态，受支持的事件可以直接切换或清除焦点。`setup.monarch_power_income_defines` 报告基础收入与焦点系数及声明来源。命令行和 Godot 国家查询的 `attributes.monthly_adm/dip/mil` 给出实时计算及缺项，`last_monthly_accruals` 给出上次入账记录；原版顾问和部分收入来源尚未绑定，完整收入保持未知。已知的自制公式可以执行月结，见 [月度君主点数](../examples/country-income/README.md)。

国家查询中的 `metrics` 提供行政、外交、军事点数，`metric_ceilings` 提供已知的增长上限。当前原版初始化规则尚未接入，这些值保留为未知；不能据此运行原版的完整开局。`setup` 报告列出 `POWER_MAX` 的基础值、声明来源及点数未知的国家。外交负值下限使用 Lua 求值后的 `DEBT_LIMIT_DIP`；缺少声明时不猜测下限。自制场景用法见 [国家点数库存](../examples/country-power/README.md)。

兼容层也可以在启动事件执行前接收外部程序提供的国家指标，例如另一套模拟器或存档读取器解析出的 `prestige`。`VanillaWorld::load_with_country_metric_values` 只绑定调用方明确给出的国家和指标，省略项继续保持未知；这条入口不推断 EU4 的生产者，也不会把外部规则写进核心定义。加载完成后，`set_country_metric_values` 可用于绑定运行中的新状态。

如果另一套游戏实例能够提供多类状态，可以使用 `VanillaExternalState` 一次性注入国家指标、每项指标的增长上限、国库与人力、已确认的阶层特权和忠诚度目标、当前省份原始自治度、已经翻译好的国家级月度现金流、已经核实的月度君主点数收入、开局陆军和舰船，以及按 EU4 省份编号给出的生产输入和税收输入。`country_metric_ceilings` 只写入提供者已经确认的动态增长上限；省略的国家或指标继续保持未知，Augustus 不会把 `POWER_MAX` 或其他基础定义自动当成每个国家的最终上限。`country_monthly_power_income` 按国家标签和 `adm_power`、`dip_power`、`mil_power` 绑定已经核实的月度收入；显式的 `null` 保持该项收入未知，未列出的国家或指标继续使用本地内容公式。它只替换收入公式，不替代点数库存和增长上限。`estate_privileges` 使用国家标签、阶层标识和特权集合绑定当前已授予特权；`Some([])` 表示明确没有特权，省略或 `null` 仍表示未知。`province_raw_autonomy_milli` 使用千分位比例绑定当前行政状态，之后的月度自治规则会从这份状态继续运行；`country_annual_tax_milli` 替换国家级年税来源；`province_annual_tax_milli` 按省份替换 `tax_after_autonomy` 的年化值，显式的 `null` 表示该省份仍未知，未列出的省份继续使用本地内容计算。税收输入使用年化千分位整数，月结时统一换算为月收入。月度现金流使用千分位整数，正数表示收入，负数表示支出；Augustus 会在税收和普通生产之后应用它，并保留每个国家的结算记录。`production_inputs` 接收已经算好的最终生产基础值；`production_value_inputs` 则接收货物数量、价值阶段修正和月结修正，由兼容层从当前 EU4 内容读取商品基础价格后生成最终输入。两种生产格式不能同时声明同一省份。开局单位在启动事件之前绑定，因此启动脚本可以读取这些单位；单位来源仍由提供者负责翻译，Augustus 不从其他游戏复制脚本或规则。该对象在启动事件之前绑定，核心只接收已经翻译好的数值；缺少的国家、指标或省份仍保持未知，错误的标签、省份编号或无港口舰船位置会拒绝整个输入。命令行可直接读取 JSON 状态包：

```json
{
  "country_metric_values": {"HAB": {"prestige": 26942}},
  "country_metric_ceilings": {
    "HAB": {"adm_power": 999, "dip_power": 999, "mil_power": 999}
  },
  "country_resources": {
    "HAB": {"treasury_milli": 120000, "manpower_milli": 5000000, "mercantilism_milli": 25000}
  },
  "estate_privileges": {
    "HAB": {
      "estate_nobles": ["estate_nobles_habsburg_disunity"]
    }
  },
  "country_annual_tax_milli": {
    "HAB": 36000
  },
  "province_annual_tax_milli": {
    "134": 6000,
    "135": null
  },
  "province_raw_autonomy_milli": {
    "134": 250
  },
  "estate_loyalty_targets": {
    "HAB": {"estate_church": 40300, "estate_nobles": 44600, "estate_burghers": 34400}
  },
  "country_monthly_cashflows_milli": {
    "HAB": 1250,
    "FRA": -300
  },
  "production_inputs": {
    "134": {
      "input": {
        "base_value_milli": 6000,
        "country_modifier_milli": 100,
        "legacy_province_modifier_milli": 0,
        "queried_province_modifier_milli": 0
      },
      "unresolved": []
    }
  },
  "production_value_inputs": {
    "135": {
      "goods_size_milli": 1000,
      "flat_modifier_milli": 0,
      "external_flat_modifier_milli": 0,
      "rate_modifier_milli": 0,
      "external_rate_modifier_milli": 0,
      "country_modifier_milli": 0,
      "legacy_province_modifier_milli": 0,
      "queried_province_modifier_milli": 0
    }
  },
  "armies": [
    {
      "owner": "HAB",
      "location": 134,
      "name": "Main Army",
      "regiments": ["western_medieval_infantry", "western_medieval_knights"]
    }
  ],
  "naval_units": [
    {"owner": "HAB", "location": 135, "unit_type": "light_ship"}
  ]
}
```

```sh
cargo run -p augustus-run --bin augustus-world --locked -- \
  --game-root "/path/to/Europa Universalis IV" \
  --external-state "/path/to/external-state.json" \
  --country HAB --days 30
```

状态包是运行时输入，不会把另一套游戏的脚本或规则复制进 Augustus；提供者负责读取、校验并翻译来源游戏的状态。

战役运行中可以调用 `VanillaWorld::set_country_monthly_cashflows` 更新已经安装的现金流表。它只替换下一次月结使用的输入，不重置税收、生产或现金流的结算次数。现金流来源未知时，应传入 `null`；核心会保留未知状态，不把它当成零。

生产执行器安装后，兼容提供者可以调用 `VanillaWorld::set_vanilla_production_inputs` 刷新下一次月结使用的省份输入。刷新不会重置已经完成的月结次数、账本或存档状态；输入仍按 EU4 省份编号绑定，不能借此创建不存在的省份。

如果提供者保留的是货物数量和生产阶段修正，可以改用 `VanillaWorld::set_vanilla_production_value_inputs`。它会重新读取当前 EU4 内容中的商品基础价格，再刷新下一次月结输入；已有月结次数和账本保持不变。没有商品价格的省份会保留为未知。

战役已经载入后，兼容提供者还可以调用 `VanillaWorld::set_estate_loyalty_targets` 更新已有阶层的目标。这个入口只修改现存国家和阶层，不会凭空创建阶层；未知标签或阶层会拒绝整个更新。

Godot 会话提供同样的 `load_world_with_content_and_external_state` 入口，参数仍使用安装目录、Mod 列表、用户目录和启动器配置，最后再传入状态包路径。

入口读取 `common/cb_types`，在 `casus_belli` 报告中列出定义来源、显式授予类型数量及尚未编译的保留条件。受支持的启动授予使用核心的有向记录；启动初始化未完成时清除部分写入并将库存标为未知。其他书签的历史授予尚未导入，同样不能判成已知没有记录。自动生成、宣战权限和战争目标尚未运行。自制样本及接口见 [宣战理由记录演示](../examples/justifications/README.md)。

开局条件和事件中的 `exists` 复用核心领土关系。海域等非陆地省份，以及已完整读取历史且未指定所有者的省份，可明确标为无主；陆地历史缺失或包含尚未处理的嵌套所有权声明时保留未知，不能据此断言某国不存在。核心宣称不赋予领土；标签查询从已登记的国家实体读取当前关系。支持范围见 [国家领土条件演示](../examples/country-existence/README.md)。

可以使用本地原版开局存档核对行政状态。在 EU4 1.37.5 中启用全部玩法 DLC、禁用所有 Mod，新建非铁人 `1444.11.11` 游戏，保持暂停，保存时取消“压缩”。然后运行：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --compare-administration-save "/path/to/baseline.eu4"
```

`administration_comparison` 分别统计所有者、核心及类型、州与领地关系、原始自治度的 `matched`、`different` 和 `unknown`，并列出已知值的差异。`unknown` 不计为通过；没有差异也不代表尚未实现的自治度或其他游戏机制已经一致。工具要求存档记录版本为 1.37.5.0、启用 22 个玩法 DLC、无模组，开局日期和当前日期均为 1444.11.11，`campaign_length = 0`。条件不符、相关字段重复或引用结构无效时会报错。存档中的省份自治缺省按其格式表示零，这不改变 Augustus 中尚未确定的初值。

该入口只读取未压缩的 `EU4txt` 存档用于对照，不导入状态或修改文件；压缩和二进制存档不受支持。它检查存档记录的条件，不能代替对实验程序和安装内容来源的确认。

仓库的 `examples/world` 是自制小世界，不需要 EU4 安装。可以将它作为 `--game-root` 使用。测试覆盖日期排序、未来条目、数值精度、国家关系、科技表与等级、修正增删、未知资源、来源追踪、重复定义与属性、分阶段计算、当前状态查询、国家汇总和错误输入，并检查兼容输入与原生输入得到相同的核心状态。

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build -p augustus-godot --locked
godot --headless --path godot --editor --import --quit
godot --headless --path godot --script res://tests/world.gd
godot --headless --path godot --script res://tests/installation.gd -- --game-root "/path/to/Europa Universalis IV" --country HAB --province 134
godot --path godot --script res://tests/world.gd
godot --headless --path godot --script res://tests/session.gd
```

最后两项界面检查分别验证真实渲染中的国家颜色，以及原有建造演示。`installation.gd` 使用指定的本地安装检查 Godot 到 Rust 的加载、日期和国家省份查询；也接受重复的 `--mod`、`--user-directory` 和 `--launcher-config`。Windows x86_64 已验证，其他平台尚未验证。报告和截图只保存在本地，不提交原版资源或其生成数据。

# 原版世界载入与时间推进

当前可以从本地 EU4 1.37.5 安装目录载入地图、国家和省份的部分开局状态。兼容模块处理文件与历史日期，将受支持的数据交给 Rust 核心；命令行和 Godot 从同一世界查询结果。

项目以终版及全部 DLC 机制为开发基线，默认玩家具备全部 DLC，不根据启动器开关选择机制。当前可以推进日历并执行税收月结；完整经济、战争、AI 和完整事件系统尚未接入，不能运行完整战役或加载 MT。兼容层可以从 `EU4txt` 文本存档或 ZIP `.eu4` 容器中的 `gamestate` 文本条目导入已经定义的启动状态；未支持的原版字段仍保留为未知。外部状态包可以在存档翻译结果上补充已经由其他兼容提供者核实的生产、现金流等输入。

已支持的 `has_dlc` 条件按这一固定基线求值，识别 22 个玩法资料片和六个原版脚本引用的旧内容包。国家开局条件、自治条件和事件编译共用名称识别；未知名称继续报告缺项或编译错误，不默认为已拥有。该判断不代表相应 DLC 的完整机制已经实现。

## 运行

省份查询的 `terrain` 由本地地形、树木和河流位图推导；无法明确分类时保留未知，`has_terrain` 遇到未知值会报错。该地图推导尚未与原版运行时逐省对照。

安装 Rust 工具链，使用自己的游戏安装目录：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --date 1444.11.11 --country HAB --province 134
```

输出 JSON 包含载入报告，以及可选的国家、省份查询和对应文件来源。模拟摘要包含陆军、舰队、舰船数量；`pending_tasks` 表示待执行的延迟任务数。国家查询分别保留科技组与历史单位组（`unit_type`）。省份基础税收、生产、人力用千分位整数表示，例如 `6000` 表示 `6`。未知国家或省份、错误日期、无效引用和不匹配的地图尺寸会报错。加载器要求安装目录版本为 1.37.5；仅修改版本声明不能使其他版本兼容。

只查询指定国家或省份时，可以加上 `--omit-load-report` 省略全量加载报告；国家、省份及其来源数据和模拟摘要仍会输出。

可以直接把自己的未压缩 EU4 开局存档作为启动状态输入：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- \
  --game-root "/path/to/Europa Universalis IV" \
  --eu4-save "/path/to/Austria1444_11_11.eu4" --days 30 --country HAB
```

该入口读取存档中的日期、玩家名单、国家资源、已知指标、君主点数库存、旗标、阶层特权、阶层忠诚度与土地份额、贸易品当前价格，以及省份当前所有者、控制者与 `previous_controller` 历史、核心国列表、`base_tax`、`base_production`、`base_manpower`、帝国归属、城市状态、原始自治度、荒废度与繁荣度指标和已建建筑，并在启动脚本执行前绑定到兼容世界。发展度字段按省份逐项覆盖历史值；荒废度与繁荣度只导入存档明确记录的数值，缺字段不补零。对存档中出现的省份，缺少 `controller` 字段或控制者标签不在当前内容中时均保持未知，不沿用历史控制者；缺少 `previous_controller` 字段时 previous history 保持未知，未知前任国家 tag 在过滤到目标内容时跳过该历史关系。核心国列表和建筑集合按存档中的省份状态导入；省份块省略 `cores` 或 `buildings` 时分别视为空集合，覆盖历史关系。存档核心列表不含完整核心／领土核心类型，因此类型保留为未知。其他未声明字段继续使用内容值，不会根据缺失字段补默认值。

存档明确授予的阶层特权会提前参与有效性判断，使其已确认的忠诚度、土地份额和特权修正能够进入阶层初始化；影响力缩放和其他未确认来源仍保持未知。省份所有者必须能在当前内容中解析，否则导入报错，避免以历史所有者掩盖存档状态。其他动态国家状态尚未导入。贸易品、建筑、核心国家和其余已支持字段按当前安装内容过滤；引用未加载的建筑或核心国家会报错。当前接受 `EU4txt` 文本文件，以及 ZIP `.eu4` 容器中使用 `EU4txt` 编码的 `gamestate` 条目；令牌化二进制 `gamestate` 暂不支持。导入的是已支持的状态子集，不是完整原版世界恢复；Augustus 的 JSON 快照仍是独立格式，不能拿 EU4 存档直接作为项目快照恢复。

存档中的贸易节点 current 值会按当前内容中的节点定义导入，并随核心快照保存；这是存档记录的当前状态，不代表已计算下一期节点更新或国家贸易收入。按省份查询时会输出 trade_node_current_milli。

`--eu4-save` 会读取省份中的未完成普通舰船订单，默认自动导入受支持的单订单省份。同省多条舰船订单须显式添加 `--import-naval-construction-queues`，或在加载基线后调用 `VanillaWorld::import_naval_construction_queues`；这份运行状态在 `--load` 恢复和时间推进前导入，不改变开局内容指纹，因此仍可用同一基线恢复旧快照。订单国家须与当前所有者及控制者一致，存档没有该省份的围城记录，港口已知，舰种存在于已加载海军定义中。每条订单的起始日期、总工期和计划日期必须一致，当前总工期须等于原始总工期；`envoy` 仅支持缺失或为 `-1`。

显式队列读取允许第二条及后续等待订单省略 `progress`，将这种已核对的零进度保存形式归一化为 `"0"`；第一条或单条订单缺少 `progress`、以及显式提供非法进度仍为未决。同省任一记录缺失其他必需字段、存在额外未支持字段、占领或围城状态、无法确认的日期或类型时，也会阻断整条队列。输出的顶层字段 `naval_construction_import` 给出导入数量及未决原因；`simulation.pending_naval_constructions` 单独统计当前待完成订单，不计入 `pending_tasks`。

程序调用者可用 `read_vanilla_naval_constructions_for_queue_import`（以及 token-map、token-resolver 版本）读取这种等待项，再传给 `import_naval_construction_queues`。现有 `read_vanilla_naval_constructions` 和默认 loader 继续保留严格字段要求；CLI 使用严格结果建立基线，再单独读取显式队列输入，保持旧快照的内容指纹。

已导入订单沿用保存的计划日期，经过该日期后才完成。保存的 `progress` 和 `cost` 只保留为诊断信息，不用于重算工期或扣费。同省队列保留存档记录的先后顺序：每日先检查队首，首个未完成订单后的订单各顺延一天；首单完成并移除后，当天重新检查下一单。不同省份独立推进。到期时创建一艘舰船及其单舰队，并清理对应订单。入队、读档和完工都不会再次收取原建造费。舰船沿用订单的国家、位置和类型，母港为建造省份；未提供的名称和士气保持未知。该通路不计算新订单价格或工期，也不从原版状态推断暂停、所有权变化或其他生产类别规则。FIFO 路径已通过自制队列及保存恢复测试；两条受控海军订单还经过原版实机逐日观察、保存、重载及完成对照。这验证了受控输入下的排队和续建，尚不覆盖自然生产命令创建多订单、其他舰型及全部游戏条件。

未完成订单、内部标识、当前完成日及 FIFO 队列标记保存在 Augustus 快照中。恢复后继续推进，与从相同输入连续运行的结果一致；完工后的订单不会重新入队。旧快照缺少 FIFO 标记时继续使用独立绝对完成日语义；原有 `enqueue_naval_constructions`、paid API 与显式新订单请求也保持这一语义。需要显式导入有序队列的核心调用者可使用 `enqueue_naval_construction_queues`，要求相应地点没有既存待完成订单。导入报告描述本次载入及显式导入结果，恢复快照后的当前订单数以 `simulation.pending_naval_constructions` 为准。`--export-external-state` 仍导出既有状态包字段，不携带这组订单数据；需要保存续建进度时使用 `--save` / `--load`，并提供相同的 EU4 存档基线。恢复已有 FIFO 队列无需再次添加导入开关，队列标记和顺延后的日期会从快照恢复。

添加 `--show-naval-constructions` 可在顶层 `naval_construction_orders` 查看恢复、提交新订单和 `--days` 推进后的当前订单明细。例如恢复后继续推进一天并省略载入报告：

```sh
cargo run -p augustus-run --bin augustus-world -- --game-root /path/to/eu4 --eu4-save /path/to/baseline.eu4 --load campaign.json --days 1 --show-naval-constructions --omit-load-report
```

每项包含稳定的 Augustus 订单 `id`、国家标签 `country`、EU4 省份编号 `province_id`、舰型 `unit_type`、当前绝对核心日 `completion_day`、日历对象 `completion_date`（`year`、`month`、`day`）及 `queue_position`。FIFO 位置从 1 开始，按各省份当前剩余队列计算，首单完成后重新编号；独立订单的位置为 `null`，即使它与 FIFO 队列处于同一省份。列表保留核心订单顺序，完工订单会移除，没有待完成订单时返回 `[]`。只有提供查询开关时才输出这个字段，也可通过 `VanillaWorld::naval_construction_orders()` 读取相同视图。

查询只读取当前模拟状态，不改变队列或快照。它列出 Augustus 模拟核心中已导入、已恢复或新提交的待完成订单；源存档中的未决记录仍由导入报告说明。`completion_date` 是查询时保存的当前排期，等待中的 FIFO 订单在后续推进时仍可能顺延，不能作为最终预计完工日。`--omit-load-report` 会省略导入诊断，但保留显式请求的当前订单明细。

省份查询中的 `attributes.province_ship_recruit_speed_modifier` 可显示静态来源 `provincial_production_size` 的基础生产分支缩放结果。识别到该来源的 `ship_recruit_speed` 属性或相关定义歧义时建立该查询；系数来自实际加载的内容，数量读取实时基础生产值。例如系数 `-0.01`、基础生产为 3 和 4 时，已知小计分别为 `-30000` 和 `-40000` micros。来源缩放按千分精度向零截断，修改基础生产后会重新求值，并随世界快照保存恢复。

这个字段仍保留其余来源、额外发展度和完整来源激活资格未接入的未决原因，`value_micros` 保持未决。缺失或变为负数的基础生产、歧义或不支持的来源属性不会当作零贡献；目前只绑定能用千分精度精确表示的系数，精度更细的系数也保留未决。该查询提供基础分支的诊断结果，不据此计算完整建船工期或安排订单完成日。

普通 galley 工期可通过 `--naval-construction-time-queries queries.json` 查询。输入使用实际加载的舰型模板标识，由调用者明确提供三项完整修正；以下是自制输入示例：

```json
[{"country":"AAA","province_id":1,"unit_type":"sample_galley","country_modifier_milli":0,"province_modifier_milli":-30,"additional_province_modifier_milli":0}]
```

查询在 `--load` 恢复及 `--days` 推进之后执行，读取当前所有者、控制者和模板类别。仅支持国家同时拥有并控制目标省份、模板类别为 `galley` 的普通工期分支；其他海军类别或跨归属状态返回未决原因。基础工期来自有效内容的 `NMilitary.GALLEY_TIME`，最低倍率来自 `NMilitary.MIN_RECRUIT_TIME_MODIFIER`，并沿用 Mod 覆盖及来源位置。缺少定义或任一修正（省略或 `null`）时，`evaluation` 为 `null`；显式零表示调用者确认该修正为零。非法定义只在使用该查询时报告带来源的错误；它不阻止其他既有功能载入。

输出 `naval_construction_time_queries` 中的 `evaluation` 给出 `modifier_factor_milli` 和 `duration_days`。倍率先计算 `(1000 + country) × (1000 + province + additional_province) / 1000`，向零截断后按最低倍率限幅，再乘基础工期并向零截断为整数天，最低一天。例如基础 365 天、最低倍率 200、三项修正 `0/-30/0` 得到 354 天，省份修正为 -40 时得到 350 天。输入和中间计算使用受检查的整数范围，超出支持范围时报错。

该查询不改变订单、国库或分类账；结果是普通分支的名义工期，FIFO 等待时间、特殊加速、跨归属倍率和完整生产资格须另行解析。现有基础生产属性只提供部分来源，查询不会将它自动当作完整省份修正。查询也不推算费用或自动提交订单。程序调用者可使用 `VanillaWorld::query_naval_construction_time`，完整已解析输入的核心计算入口为 `calculate_vanilla_naval_construction_time`。

普通 galley 成本可通过 `--naval-construction-cost-queries queries.json` 查询；它读取有效内容的 `NMilitary.GALLEY_COST`，当前支持非负整数货币单位，并检查乘以 1000 后的 `i32` 范围；小数定义不在支持范围内。三项完整成本修正、数量与限额由调用者解析，以下是自制输入：

```json
[{"country":"AAA","province_id":1,"unit_type":"sample_galley","country_modifier_milli":0,"additional_country_modifier_milli":0,"province_modifier_milli":0,"naval_quantity_milli":10000,"naval_limit_milli":17000}]
```

查询在恢复和推进后执行，并在保存前完成校验；输出 `naval_construction_cost_queries` 数组。普通分支要求当前国家与省份所有者、控制者一致，模板类别为 `galley`，且显式数量不超过正限额。未知身份或非海军模板报错；其他海军类别、归属不匹配或未知、缺定义或缺任一输入返回 `evaluation: null` 与未决原因。零限额和超限保留未决，不推断价格放大；显式负数量和负限额属于本接口拒绝的输入域。`naval_quantity_milli` 与 `naval_limit_milli` 使用相同千分单位，当前入口不会根据已载入舰船或待建订单自动推导这些值。

结果中的 `modifier_factor_milli` 为 `max(200, 1000 + country + additional_country + province)`；`cost_milli` 为基础千分成本乘倍率、除以 1000 向零截断后，按最低 1000 milli 限幅。倍率下限 200 与金额下限 1000 是本普通分支的固定规则，不使用工期的最低倍率定义代替。基础成本 10000、三项修正为零、数量 10000 不超过限额 17000 时，结果为 10000 milli。此处的通用修正输入名称不声明其原版来源已经自动解析。

该查询保留有效定义的 Mod 覆盖和来源。非法定义只在使用成本查询时带来源报错；查询及解析失败不会创建快照。查询不扣费或入队；调用者可将解析成功的 `cost_milli` 放入下述显式订单请求，并继续单独提供完成日和资格。程序入口为 `VanillaWorld::query_naval_construction_cost`，核心入口为 `calculate_vanilla_naval_construction_cost`。本功能只计算普通限额内成本，不涵盖超限放大、特殊 flags、其他舰型或完整生产资格，也不证明原版负值或溢出行为；支持域内使用受检查的整数计算。

海军力限也可通过 `--naval-force-limit-queries queries.json` 查询。该入口只接受调用者完整解析的 `base_milli`、`flat_modifier_milli` 和 `percentage_modifier_milli`，显式 `0` 是有效输入，缺项返回 `evaluation: null` 与对应 `naval_force_limit:missing_input:*`。它只校验国家标签并委托 `calculate_vanilla_force_limit`，不会把 `country_attributes` 的部分 subtotal、发展度、自治、建筑或港口状态自动升级成完整限额，也不会写入快照或修改建船报价。程序入口为 `VanillaWorld::query_naval_force_limit`。

陆军力限也可通过 `--land-force-limit-queries queries.json` 查询。输入字段与海军力限查询相同，必须由调用方完整解析 `base_milli`、`flat_modifier_milli` 和 `percentage_modifier_milli`；显式 `0` 是有效输入，缺项返回 `evaluation: null` 与对应 `land_force_limit:missing_input:*`。该入口只校验国家标签并委托共享核心的 `calculate_vanilla_force_limit`，不会把 `country_attributes` 的部分 `country_land_forcelimit_flat`、省份发展度、自治、建筑或其他来源小计自动升级成完整陆军力限，也不会写入快照或改变世界状态。程序入口为 `VanillaWorld::query_land_force_limit`。

成本和工期查询的修正字段对应以下内容属性；表中的值均指完整修正总量，单位为千分位。字段名称沿用现有 JSON 契约。海军和陆军力限查询的三个输入由调用者完整解析，不对应这里的内容属性表。

| 查询 | 输入字段 | 对应内容属性 |
| --- | --- | --- |
| 成本 | `country_modifier_milli` | 国家 `galley_cost` |
| 成本 | `additional_country_modifier_milli` | 国家 `global_ship_cost` |
| 成本 | `province_modifier_milli` | 省份 `local_ship_cost` |
| 工期 | `country_modifier_milli` | 国家 `global_ship_recruit_speed` |
| 工期 | `province_modifier_milli` | 省份主修正容器中的 `ship_recruit_speed` |
| 工期 | `additional_province_modifier_milli` | 省份另一修正容器中的 `ship_recruit_speed` |
| 海军力限 | `base_milli` | 调用者解析的完整基础力限 |
| 海军力限 | `flat_modifier_milli` | 调用者解析的完整平坦修正 |
| 海军力限 | `percentage_modifier_milli` | 调用者解析的完整百分比修正 |
| 陆军力限 | `base_milli` | 调用者解析的完整基础力限 |
| 陆军力限 | `flat_modifier_milli` | 调用者解析的完整平坦修正 |
| 陆军力限 | `percentage_modifier_milli` | 调用者解析的完整百分比修正 |

工期的两个省份输入来自不同容器，不能将同一份小计重复传入。属性名的对应关系不表示来源已经自动聚合：`province_ship_recruit_speed_modifier` 仍只包含已接入的生产来源，国家陆军和海军力限属性也保留未决来源，三者的 `known_subtotal_micros` 都不能作为完整输入。查询继续要求调用者提供完整值；只有确认某项总量为零时才显式传入 `0`，省略或 `null` 保持未决。

新舰船请求可由调用者提供明确费用和完成日，通过 `--naval-constructions requests.json` 提交。文件是 JSON 数组，例如以下自制输入：

```json
[{"country":"AAA","province_id":1,"unit_type":"sample_light_ship","completion_day":3,"cost_milli":5000}]
```

请求在恢复 `--load` 之后、推进 `--days` 之前执行，每次调用都会提交新请求。`completion_day` 是从该世界起始日计算的绝对核心日，必须晚于当前日；`cost_milli` 是调用者明确提供的非负金额。整批校验成功后，按国家合并费用并采用已有国库 Subtract 语义扣减余额或累计待定负增量，同时分配订单标识；任一项失败都不扣费或入队。本操作不写入 `CountryLedger`。输出 `submitted_naval_constructions` 给出本次新订单标识。恢复已有建造进度时只提供 `--load`，无需再次传入这份请求文件。

需要在同一港口提交多笔有序订单时，使用 `--naval-construction-queues requests.json`。它接受相同的请求数组，并按数组中各港口订单出现的顺序组成 FIFO；不同港口独立推进。例如以下自制输入在同一港口提交两笔明确报价的订单：

```json
[
  {"country":"AAA","province_id":1,"unit_type":"sample_light_ship","completion_day":3,"cost_milli":5000},
  {"country":"AAA","province_id":1,"unit_type":"sample_heavy_ship","completion_day":2,"cost_milli":2000}
]
```

`--naval-construction-queues` 与 `--naval-constructions` 互斥。所有涉及的港口必须没有既存待完成订单；已有独立订单或 FIFO 队列都会使整批提交失败。本入口不会将已有独立订单提升为队列，也不追加已有队列。同一批次中只有一笔订单的港口保持独立完成日表示。`completion_day` 是提交时提供的当前绝对完成日，等待项会随 FIFO 等待逐日顺延；上例从 day 0 提交并连续推进时，第一单在 day 3 完成，第二单在 day 4 完成。所有价格仍由调用者明确给出，提交、扣费和 FIFO 标记作为一次原子事务完成，任何项失败都保持订单、标识和资源不变。

程序调用者可使用 `VanillaWorld::submit_naval_construction_queues` 或 `submit_naval_construction_queues_with_ledger`；对应核心入口为 `World` / `InitialWorld` 的 `enqueue_paid_naval_construction_queues` 与 `enqueue_naval_construction_queues_with_expenses`。保存与恢复使用现有快照中的队列状态；恢复进度时省略请求文件，避免再次提交。`--show-naval-constructions` 可查看提交和推进后的当前 FIFO 位置。

需要显式取消待完成订单时，使用 `--cancel-naval-constructions ids.json`，文件内容是 Augustus 订单 ID 的 JSON 数组，例如 `[0, 2]`。取消在 `--load` 恢复之后、`--days` 推进之前执行；订单 ID 必须唯一且仍处于待完成状态，空数组为空操作。整批校验通过后才会删除订单及对应 FIFO 标记，队列位置会按剩余订单自然重编号；剩余订单已经保存的当前完成日不会回拨。取消不会退还提交时的费用，也不会反向修改 `CountryLedger`。未知、重复或已完成 ID 会使整批操作失败，并保持订单、FIFO 标记、资源、分类账和快照文件不变。该选项与 `--naval-constructions`、`--naval-construction-queues` 及 `--naval-construction-ledger` 互斥；可以单独配合 `--load`、`--days` 和 `--show-naval-constructions` 使用。核心调用者可直接使用 `World::cancel_naval_constructions`、`InitialWorld::cancel_naval_constructions` 或 `VanillaWorld::cancel_naval_constructions`。

需要将本次费用登记为舰船建设支出时，在普通请求或 FIFO 请求选项之外同时传入 `--naval-construction-ledger`；单独提供分类账开关会报缺少请求文件。此模式使用 `EXPENSEFLEET` 类别，要求相关国家国库余额已知；它将费用计入分类项、来源、累计及辅助支出向量，同时扣减国库并入队。整批操作原子提交，分类账溢出或任何订单错误都不会留下部分扣费或账目。没有分类账时，沿用即时支出 API 的空账初始化规则，只记录本次费用，不重建历史账目。即时费用不增加待结算余额，保存恢复和完工也不会再次记账。恢复已有订单时无需再传请求选项和分类账开关。

适配层要求加载的舰种、已知港口、订单国家与当前所有者及已载入控制者一致，并拒绝已有待完成订单或存在导入未决军用订单的省份。普通请求还拒绝本批重复使用的省份；显式 FIFO 请求允许这种重复，并保留各港口的原始请求顺序。导入报告无法确认整个订单输入时，也拒绝新请求。这些入口使用外部明确提供的金额和完成日，不推算原版价格、资格、自动借贷或工期。已知国库沿用增减限幅；未选择分类账模式时，未知国库保留未知余额并累计待定负增量。完整原版报价和队列生命周期规则仍需继续接入。

如果另一个兼容提供者还掌握存档中没有的运行时输入，可以把状态包作为覆盖一起传入。存档提供玩家身份、资源、阶层、自治度和单位等基线；状态包只覆盖自己声明的字段，未声明的存档字段继续保留：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- \
  --game-root "/path/to/Europa Universalis IV" \
  --eu4-save "/path/to/Austria1444_11_11.eu4" \
  --external-state "/path/to/runtime-overlay.json" --days 30
```

国家级嵌套字段按国家和字段合并；非空的陆军或舰船列表表示提供者给出的完整单位快照。恢复 Augustus 快照时仍须提供生成快照时相同的存档和状态覆盖。

如果需要把存档中的已翻译状态交给另一套内容或 Mod，可以额外导出状态包。导出文件不会覆盖已有文件：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- \
  --game-root "/path/to/Europa Universalis IV" \
  --eu4-save "/path/to/Austria1444_11_11.eu4" \
  --export-external-state "/path/to/external-state.json"
```

之后用 `--external-state` 载入状态包，并按需要用 `--player` 指定控制身份。状态包只包含兼容层已经翻译的运行时输入，未支持的 EU4 字段不会被伪装成完整状态。

从 EU4 存档启动后，也可以继续使用同一份 EU4 存档作为内容基线恢复 Augustus 快照：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- \
  --game-root "/path/to/Europa Universalis IV" \
  --eu4-save "/path/to/Austria1444_11_11.eu4" --days 30 \
  --save augustus-from-eu4.json
cargo run -p augustus-run --bin augustus-world --locked -- \
  --game-root "/path/to/Europa Universalis IV" \
  --eu4-save "/path/to/Austria1444_11_11.eu4" \
  --load augustus-from-eu4.json --days 30
```

恢复时必须使用生成快照时的同一份 EU4 存档和内容；存档或本地内容变化会被内容指纹拒绝。

需要读取目录 Mod 时，可重复提供 `--mod`，并用 `--launcher-config "<EU4 用户目录>/dlc_load.json"` 按启动器的 `enabled_mods` 筛选；Godot 启动参数也接受这些选项及 `--user-directory`。启用列表中的每个 Mod 仍须提供其目录。默认使用安装目录内可用的全部 DLC，不读取启动器的 DLC 禁用列表。

省份查询的 `metrics.devastation` 和 `metrics.prosperity` 使用千分位整数，`null` 表示当前数值未知。存档明确记录的荒废度和繁荣度会导入；省份事件支持荒废度、繁荣度的字面条件与增减效果。荒废度静态修正按当前值比例应用；繁荣度的原版静态修正缩放尚未确认，其商品数量影响保持未决。月度荒废度恢复和繁荣度增长尚未实现。开局脚本未能完整执行时，不会把初始化的零当作最终值。

国家查询的 `metrics.stability` 使用整数，范围为 −3 到 3，`null` 表示未知。已接入国家作用域下的字面量 `stability` 条件与 `add_stability` 效果，小数部分向零截断；正负属性分别读取 `positive_stability` 和 `negative_stability`，税收与人力查询从省份所属国家读取当前修正。1444.11.11 的无歧义历史使用启动前零值，随后执行已支持的启动内容。尚未验证的历史赋值、间接历史操作、其他开局日期或失败的启动执行保留为未知；未支持国家间稳定度比较、购买稳定度及相关成本系统。

`--country` 只选择查询对象。需要明确控制身份时使用 `--player HAB`，可以重复提供多个玩家国家；其余已载入国家标记为 AI。`--ai-only` 明确指定没有玩家国家，不能与 `--player` 同用。名单在启动效果执行前绑定，身份随存档保存；读档时使用相同的开局名单，恢复保存时的当前身份。不提供名单时身份未知，需要 `ai` 判断的启动内容会报告缺项。Godot 界面会为待选事件暂停时间推进并显示当前可用选项；本地化文本尚未接入，暂显示事件和选项键。

Godot 界面使用 Godot 4.7.2：

```sh
cargo build -p augustus-godot --locked
godot --path godot
```

选择安装目录、开局日期和玩家国家标签后点击“载入开局”。玩家标签支持逗号分隔，例如 `HAB`；标签会在启动效果执行前绑定，其余国家自动使用 AI 身份。留空时保留“控制身份未知”的诊断行为。待选事件会暂停自动推进和日历推进，显示当前实体、事件键和可用选项；选择后继续处理后续事件。地图显示当前已载入的领土颜色；国家下拉框可查看领土、首都、文化、宗教、政体、统治者、外交关系、已载入的科技等级与重商主义，点击地图可查看省份、历史记载建筑、商品基础价格、贸易节点与下游连接，以及永久历史修正的数值声明。右侧面板可以滚动；点击省份后自动显示省份详情。也可以在启动时提供目录：

```sh
godot --path godot -- --game-root "/path/to/Europa Universalis IV" --date 1444.11.11 --player HAB
```

未载入的值显示为“未载入”。界面列出未应用历史字段的数量和存在历史文件歧义的国家数；这些内容仍需实现或验证。重新载入失败时保留原来的世界。

## 日期、月结与存档

`--eu4-save` 会恢复存档中的 `multiplayer_random_seed` 和 `multiplayer_random_count`，并在启动脚本运行前重建随机流。它与 `--seed` 互斥；新开局使用 `--seed` 指定种子。

载入后默认暂停。可以点击“推进一天”“至下月”，或点击“运行”连续推进、“暂停”停止。“保存”“读取”使用 Godot 用户数据目录中的 `augustus-world.json`，不修改原版安装或原版存档。保存会替换这个项目存档；读取后保持暂停。

命令行可以推进日期，在中途保存并继续：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --country HAB --seed 8351375 --days 45 --save augustus-world.json
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --country HAB --load augustus-world.json --days 320
```

新 EU4 世界的随机种子通过 `--seed` 指定，默认是 `0`；显式值按 EU4 Windows 命令行的有符号 32 位十进制 `atol` 规则转换：前导空格和制表符会被忽略，数字前缀之后的内容不参与转换，正负溢出分别饱和到 `INT_MAX` 和 `INT_MIN`，再按对应的无符号位型初始化 RNG。例如要使用存档 seed `4,149,563,735`，传入 `--seed -145403561`。恢复时以快照保存的随机状态为准。EU4 快照会保存种子、抽取计数、624 项状态数组和当前索引；旧版通用快照仍可读取。`simulation.eu4_random` 报告当前种子和计数。`--date` 始终指定开局日期，恢复时须与保存时一致。命令行拒绝覆盖已有文件。存档核对受支持内容、开局数据和所选月度系统，内容变化、错误存档或不匹配的世界会报错。默认开局推进 365 天到达 `1445.11.11`，跨过十二次月结。

已验证的范围覆盖原版随机状态的初始化、单值生成、1,000 值批量生成及按保存计数重建。具体游戏系统如何把随机值映射为区间结果，以及非固定种子的时钟输入，仍需原版对照；因此这不代表所有随机消费者已与 EU4 一致。

日历每年 365 天，二月固定 28 天。每月首日执行一次月结，开局当天不结算；同日任务先按登记顺序执行，再计算税收。标准税收系统按当前所有权汇总省份 `tax_after_autonomy` 年化属性，除以十二并转为千分位货币，向零截断。这个执行顺序与取整约定已通过自制样本验证，仍需原版运行对照。

国家查询的 `last_monthly_tax` 保存最近一次结算的已知部分、完整收入、实际入账、前后余额和缺项；启用生产执行器后，`last_monthly_production` 保存按国家汇总的年度值、月度值、逐省结果、国库前后余额和缺项。只有完整收入和原有余额都已确定才入账。税收缺项会使之后的余额保持未知，不能把旧余额或部分收入当作当前准确余额。原版开局的国库及部分税收来源尚未确定，因此当前原版推进会产生有缺项的记录；生产、贸易、维护、借贷和人力结算尚未全部接通。

Rust 原生系统可以实现 `MonthlySystem`，在开局前整体替换月度执行者；存档记录系统标识和版本，恢复后需要同一实现。核心提供 `MonthlyPipeline`，按“世界阶段、并行国家阶段、串行国家阶段”的固定顺序组合多个月度 provider；它只负责编排，不实现任何特定 EU4 业务。核心自带的 `MonthlyEventRules` 可以执行兼容层已经翻译好的月度事件候选，使用与日、年度事件规则相同的普通组和加权组语义；它不替兼容层猜测 EU4 月度业务的具体顺序。示例见 `crates/augustus-core/tests/calendar_taxation.rs`、`crates/augustus-core/tests/monthly_events.rs` 和 `crates/augustus-core/tests/monthly_pipeline.rs`。兼容输入与原生输入使用同一个核心和调度入口，当前接口尚不是稳定 Mod SDK。

每日业务也有同样的替换入口。实现 `DailySystem` 后，在开局前调用 `select_daily_system`；核心会在每个推进日调用一次 `tick`，再处理当天任务和月度执行器。存档只保存执行器标识，恢复后必须重新提供相同标识的实现；没有选择每日执行器时，现有 `advance` 行为不变。核心自带的 `DailyEventRules` 提供了翻译后事件候选的通用执行器：普通组先冻结资格再按声明顺序派发，加权组只选一个正权重候选，事件目录自己的 once 记录继续负责重复触发限制。它只接收已经翻译好的事件候选，不替兼容层猜测 EU4 的具体日常规则。

年度业务也有独立的 `YearlySystem` 替换入口。实现后，在开局前调用 `select_yearly_system`，推进时通过 `advance_with_yearly_system` 或 `advance_with_systems_and_yearly` 提供相同实现；跨年当天先执行月度系统，再执行年度系统。核心提供 `YearlyPipeline`，按“世界年度阶段、国家年度阶段”的固定顺序组合多个 provider；它只负责编排，不实现任何特定 EU4 业务。年度执行器标识会写入项目存档，恢复时必须匹配，未选择年度执行器时不会改变原有推进路径。核心自带的 `YearlyEventRules` 可执行兼容层已经翻译好的年度事件候选，使用与 `DailyEventRules` 相同的普通组和加权组语义。当前原版兼容入口只转换 `on_yearly_pulse` 中直接声明的 `events` 列表；回调中出现未支持字段时会停用该组年度派发并报告来源，年度事件定义编译失败时则保留开局事件目录。`VanillaWorld::yearly_event_rules()` 只读返回加载器已翻译的规则（若可用）；调用者可先克隆规则，再与 `CountryLedgerAnnualRollover` 等 provider 组合为 `YearlyPipeline`。选择自定义组合后，推进时须通过 `advance_with_yearly_system` 或 `advance_with_systems_and_yearly` 显式传入同一 pipeline；普通 `advance()` 不会自动发现调用者创建的 provider。恢复自定义组合存档前，也须先选择具有相同 ID 的 pipeline。这个接口只提供阶段边界和顺序，不代表 EU4 的年度规则已经全部翻译，也不证明原版各阶段的先后顺序。

原生内容也可通过 `Taxation::with_country_annual_attribute` 指定国家级年化税收属性。EU4 加载器会将 `all_nations.global_tax_income` 用于已知拥有省份的国家；当前皇帝还会按 HRE 成员中已选 `free_city` 改革的国家数，计入 `free_cities_in_hre.global_tax_income`。无领土国家的特殊资格尚未接入，年税保留为未知。未知国家属性会阻止实际入账，已知省份小计仍保留；配置与账项参与保存和恢复校验。原版 HRE 状态变更后的缓存刷新时序尚未完整验证。

## 当前范围

载入固定地图的省份定义和位图，区分陆地、海域、湖泊、荒地与固定地图未使用的定义。国家定义提供标识和颜色；国家历史处理首都、主文化、宗教、政体和科技组。省份历史处理所有者、控制者、核心、文化、宗教、商品、基础税收、生产、人力、城市状态和帝国归属。

先处理无日期的基础字段，再按时间顺序处理不晚于所选日期的条目；同日条目保持文件中的顺序。历史派生字段缺失时保留未知状态。同一国家有多个历史文件时，报告所有候选文件，暂不选择其中一个；首都必须属于该国才能绑定核心关系。

未支持的历史字段记录次数和一个实际文件位置，不执行其效果。因此，当前显示的国家数量和领土信息不能作为完整原版开局已经一致的证明。尚未接入军队、完整经济结算、事件、继承和外交操作等游戏系统。当前入口只读取原版目录，Mod 组合仍使用独立的[文件检查工具](EU4_CONTENT.md)。

## 人物与外交

国家历史中的君主、继承人及配偶会载入核心，保留姓名、王朝、出生日期、历史死亡日期、三项能力、宣称强度、摄政和来源国家等受支持字段。缺少的能力值保持未知；已超过声明死亡日期或尚未出生的条目不作为当前角色，报告保留其来源。嵌套的军事领袖、人物性格和未支持字段仍会报告。

外交历史载入同盟、王室联姻、保障、警告、贸易联盟成员、属国关系，以及两个皇帝职位。国家历史中的 `add_truce_with` 按历史日期和 `TRUCE_YEARS` 导入为双向停战，开局前已退役的关系视为已知不存在；未声明的国家配对仍保持未知。属国类型须在本地定义中存在。无向关系统一两国顺序；保障、警告、贸易联盟和属国关系保留方向。重复关系合并为一个核心状态，同时保留各条来源。核心拒绝自我关系、无效国家引用、多个宗主或循环属国关系。兼容事件在国家作用域中支持 `set_emperor = yes`，直接更新帝国皇帝职位；正常选举流程仍未接入。

当前日期选择采用包含开始日、不包含结束日的区间；各文件中的皇帝变更统一按日期排序。这些输入规则通过自制用例和本地原版文件载入检查，尚未完成与原版运行状态的逐项对照。

使用 `--country` 查询该国人物和外交，使用 `--diplomacy` 查询全局关系、停战到期日及皇帝职位：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --country FRA --diplomacy
```

人物与外交已纳入核心存档及恢复校验，已有建造演示存档仍可读取。历史中的未来死亡或关系结束日期仅用于选择开局，不会安排战役中的后续变化；实际继承、关系变化和相关结算尚未实现。

## 建筑、商品与贸易网络

EU4 存档中已支持的贸易节点运行值会在 --economy 输出各节点的 runtime_state 中显示，数值以千分位整数表示；支持 current、local_value、outgoing、value_added_outgoing、retention、p_pow、max、collector_power、collector_power_including_pirates、pull_power、retain_power 和 highest_power，未提供的字段为 null。它们是存档中的当前状态，不表示已完成下一期节点刷新或国家贸易收入结算。

建筑目录读取明确声明的费用与工期；费用采用千分位整数，工期保留月数，不换算成固定天数。省份历史中以建筑标识为字段的 `yes` / `no` 条目，按开局历史顺序增删建筑。商品目录与价格目录合并为核心定义，保留兼容层观察到的运行时索引、基础价格和声明的金矿类别。运行时索引从 1 开始，按选定内容中 `common/tradegoods` 的定义顺序分配；核心把它当作可选的外部标识，不用它推导动态价格。缺少的费用、工期、价格或类别保持未知，不补默认值。

贸易节点保留位置省份、成员省份、下游连接，以及声明的内陆和终点标记。核心检查引用、重复归属、自我连接和循环网络。输入中的重复定义会报错，当前入口不猜测其覆盖顺序。省份没有历史文件时，建筑查询返回 `null`；有历史文件但没有受支持的现存建筑条目时返回空列表。这份列表表示历史输入结果，尚未执行建筑保留条件。

使用 `--economy` 查询建筑与商品目录、完整贸易网络；报告的 `economy` 字段给出数量、定义来源、建筑历史的最终声明位置和未应用字段。原有省份 JSON 字段继续保留，新增 `buildings`、`trade_node`、`trade_node_outgoing` 和 `trade_good_definition`。查询单个省份时还会给出 `province_trade_good_base_price_milli` 和 `province_trade_good_price_milli`：前者是 `common/prices` 中声明的商品基础价格，后者优先反映存档或外部状态提供的当前价格；两者都不代表贸易结算已经完成。Rust 调用方可以使用 `VanillaWorld::calculate_vanilla_province_production_base_value`，在明确提供货物数量及修正项时复用当前价格；没有已知商品价格时返回未知，不补默认值。

建筑定义会保留标准建筑与要塞的角色及要塞等级。要塞不参与普通省份属性的建筑修正汇总；防御区域、维护和战斗路径尚未接通。存档提供的 `change_price.current_price` 会作为运行时价格保存；没有运行时价格时，生产值阶段继续使用内容基础价格或显式价格影响，缺失值不会被补成默认市场价。脚本 `change_price` 当前支持 `duration = -1`：影响按商品和 key 保存，在下一次月度价格刷新时参与价格重建并更新已绑定的生产值输入。有限期限的脚本效果及到期清理尚未实现；EU4 存档中的 `expiry_date = 1821.1.2` 影响会恢复，有限期限或字段不全的影响保持未决，该商品会沿用存档当前价，不做部分重算。

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --province 134 --economy
```

这些数据与人物、外交一样进入同一个核心及存档，原有建造演示存档仍可读取。部分建筑与商品的省份数值修正已进入下述属性计算；自动生成生产上游、建造与保留条件、回调和贸易收入仍未全部接入，但兼容层提供明确输入时，月度生产执行器会把逐省结果写入国家国库并保留结算记录。动态价格可以由外部状态按运行时索引提供，兼容层会在生产值阶段重建有效价格；绘制路线的路径和控制点也未接入。核心已提供贸易节点中单个国家行的生产量、价值份额和收入 pass 计算，以及贸易传播、保留比例、路线权重归一化、候选排序和价值下推算子；兼容层仍需确定完整国家行集合及资格，并提供省份产出、国家权力、路线和政策等上游状态，才能组成完整贸易结算。未处理的定义字段会记录在报告中。显示的基础价格不能当作实际商品收入，贸易连接也不能当作贸易结算已经完成。

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

科技组、三条科技表和 `common/units` 模板从本地定义读取，科技等级按表内声明顺序从 0 编号。每级的 `enable` 单位模板名称也按声明顺序载入，并可从 `--technology` 查询结果查看。单位模板可用 `--unit <文件名>` 按文件主名查询，其类别和声明字段按原顺序保留。仅在 `1444.11.11`，使用国家科技组明确声明的 `start_level` 设置行政、外交和军事等级；没有科技组或选择其他日期时，等级保持未知，不根据年份猜测。国家查询会列出技术表启用且单位组匹配的候选；每个类别只有一个候选时自动选中，多候选类别保持未选。单位解锁条件、升级费用、累积效果和研究过程尚未执行。

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

省份查询的 `attributes` 包含 `development`、`tax_basis` 和 `manpower_contribution`；`development` 为税收、生产与人力基础值之和，可用于省份作用域的 `development = n` 条件。任一基础值未知时，总发展度也保持未知。Godot 显示税收与人力的“已接入部分，自治前”计算，并列出来源和缺项。税收基数从基础税收开始，人力贡献按本地 `NMilitary.BASE_MP_TO_MANPOWER` 的明确声明换算为人数。月结使用另一个自治后属性 `tax_after_autonomy`；完整国家人力上限及可用人力结算尚未建立。

当前组合城市状态、明确的占领状态、历史记载建筑、商品的 `province` 块、永久省份与所有者历史修正，以及文化待遇、宗教直接效果和已接入的宽容度来源。税收接入固定增量 `tax_income`、省份 `local_tax_modifier` 与所有者 `global_tax_modifier`；人力接入省份 `local_manpower_modifier` 与所有者 `global_manpower_modifier`。其他静态修正仍只是目录声明，未根据其名称自动生效。建筑保留条件尚未执行，参与计算的建筑仍是历史载入结果。

文化组从本地定义读取，接受文化的增删按所选历史日期处理，文化联盟按已接入的国家等级规则初始化。主文化、接受文化及联盟接受的文化不应用基础文化惩罚；同组和异组的未接受文化分别选择 `same_culture_group`、`non_accepted_culture` 中的数值。宽容度从 `base_values`、当前国教直接效果、已选政体改革、合法性、理念、阶层、运行修正和宗教派别计算；省份与国教的关系决定采用本国、异端或异教宽容度。正负宽容度分别按 `tolerance` 和 `intolerance` 静态定义缩放，当前接入税收、人力修正，以及 `trade_goods_size_modifier` 的宽容度小计。商品数量修正会汇总已绑定的省份、国家、宗教、政体、理念、阶层、建筑、商品、行政状态、城市和占领来源；`devastation` 省份指标按定义的尺度应用其静态修正。存档明确记录的 `prosperity` 当前值已导入，省份事件可查询或增减该指标；繁荣度静态修正的缩放尚未确认，相关商品数量来源保持未决。存档中的活动州敕令按当前所有者和地区绑定到相应省份，已接入的修正参与对应属性计算，未支持的效果保留为未决；运行中更换敕令、改变所有权或地区后的同步尚未接入。围城及其他运行来源仍未绑定，商品数量小计尚未作为完整输入接入月度生产。科技及其他未绑定来源继续报告为未决，不把部分宽容度当成完整税收或生产结果。宗教直接效果只应用当前国教的 `country` 块和省份宗教的 `province` 块中已支持的属性；不自动应用次要宗教或转换回调。`report.attributes.religion_definitions` 给出已确定的宗教定义数量；定义和属性重复时保留来源与缺项。

每条计算返回基础值、各阶段的固定增量、比例之和、输出与贡献来源。数值均使用百万分位整数；`known_subtotal_micros` 是已接入部分的结果，`unresolved` 列出缺项。只要存在缺项，`value_micros` 就是 `null`。基础值未知时，已知部分结果也为 `null`。国家查询的 `province_attribute_totals` 按当前所有权汇总省份贡献，同时保留缺项；尚有省份归属未确定时，只给出已知领土的小计，完整总量保持未知，明确无主的省份不会造成这一缺项。它不包含国家固定人力等尚未接入的来源。`country_naval_forcelimit_flat` 也会保留已由存档绑定的 `shipyard` 或 `grand_shipyard` 建筑贡献；这类贡献仍属于部分小计，其他来源未决时 `value_micros` 继续为 `null`，不能替代完整海军力限输入。

完整自治与州规则、政体对文化待遇的影响、宽容度与宗教机制、理念、阶层、科技效果及其他来源还需实现和核对。当前结果不能作为完整原版税收、人力上限或收入。原有 `resources.manpower_milli` 表示可用人力，继续保持未知，不用省份贡献代替。

## 地区与自治

省份作用域的 `has_state_edict` 查询读取已导入的活动敕令，与其省份 modifier 共用同一状态。敕令定义中的未支持顶层字段会记录在 `report.setup.unapplied_definition_fields`，每项的首个来源保存在 `report.setup.unapplied_examples`。

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

国家历史中的字面 `national_focus = ADM/DIP/MIL` 已进入共享人物状态，受支持的事件可以直接切换或清除焦点。`setup.monarch_power_income_defines` 报告基础收入与焦点系数及声明来源。命令行和 Godot 国家查询的 `attributes.monthly_adm/dip/mil` 给出实时计算及缺项，`last_monthly_accruals` 给出上次入账记录。顾问类型定义会进入 `setup.advisor_types`；外部状态和未压缩 EU4 存档可以绑定当前顾问槽，核心按顾问类别选择第一个匹配顾问并保留其技能。国家查询的 `advisors_known` 表示顾问列表是否已由兼容层确定，`advisors` 的空数组表示已确认没有在职顾问，`null` 且 `advisors_known=false` 表示仍未知。`advisor_slots` 可报告已单独确定的类别槽；未出现的类别仍未知，值为 `null` 表示该槽已确定为空。新战役的顾问生成、力量投射及其他收入来源尚未全部接通，因此完整收入仍可能保持未知。已知的自制公式可以执行月结，见 [月度君主点数](../examples/country-income/README.md)。

国家查询中的 `metrics` 提供行政、外交、军事点数和已知的 `power_projection`，`metric_ceilings` 提供已知的增长上限。当前原版初始化规则尚未接入，这些值保留为未知；不能据此运行原版的完整开局。`setup` 报告列出 `POWER_MAX` 的基础值、声明来源及点数未知的国家。外交负值下限使用 Lua 求值后的 `DEBT_LIMIT_DIP`；缺少声明时不猜测下限。已知力量投射达到 50.000 时，月度三项君主点数公式会增加 1；缺少该指标仍保持未决。自制场景用法见 [国家点数库存](../examples/country-power/README.md)。

兼容层也可以在启动事件执行前接收外部程序提供的国家指标，例如另一套模拟器或存档读取器解析出的 `prestige`。`VanillaWorld::load_with_country_metric_values` 只绑定调用方明确给出的国家和指标，省略项继续保持未知；这条入口不推断 EU4 的生产者，也不会把外部规则写进核心定义。加载完成后，`set_country_metric_values` 可用于绑定运行中的新状态。

存档中国家当前 `native` 块的 `adm_dev`、`dip_dev`、`mil_dev` 分别导入国家指标 `native_adm_development`、`native_dip_development`、`native_mil_development`。三项使用千分位整数，例如 `dip_dev=1.146` 对应 `1146`，可通过 `--country TAG` 的 `metrics` 查询。块内缺失的分量为 `null`；没有当前 `native` 块时不声明这组输入，历史块中的同名字段也不作为当前值。重复块或分量、格式错误及超出有符号 32 位千分整数范围的值会拒绝导入。

外部状态包的 `country_metric_values` 可提供同名指标，逐项覆盖存档值；显式 `null` 表示该分量未知。提供任一项时注册这三项指标定义，未提供的分量仍未知；已经注册的指标可通过 `set_country_metric_values` 或核心数值接口更新，并随 Augustus 快照保存恢复。`--export-external-state` 保留这些值，导出的状态包可直接重新载入。这组数据表示存档记录的发展分量；完整机制资格、发展值月度增长及额外发展度如何参与其他计算仍待接入，建船速度查询继续保留相关未决项。

兼容层还提供 `calculate_vanilla_province_native_growth_cache`，用于计算已经由调用方解析好资格、荒废度、净月度荒废变化、基础进度、领土加成、荒废贡献分母和所选部落发展分量后的省份增长缓存阶段。计算先用月度变化应用前的荒废度判断 `100000` 上限，再处理高荒废比例；荒废贡献分母为零时使用已观测的 `-1` 比例哨兵，所选分量低于 `1000` 时直接保留进度，否则按分量归一化。结果只返回诊断值和缓存候选，不修改省份、国家指标或月度系统。迁移资格、来源生产、国家第二路增长、缓存清理和保存恢复仍由调用方负责，未由这个计算器推断。

`calculate_vanilla_province_native_growth_application` 可在缓存阶段之后复现两路写入的顺序：正缓存先增加所选分量，再用写入后的分量计算国家增长；分量低于 `1000` 时国家增长直接写入，缓存为零时跳过省份写入但继续国家阶段。该方法同样只消费调用方已解析的显式输入，不自动推进存档指标或月度系统。

`calculate_vanilla_native_migration_cost` 消费调用方解析的三项 power define、目的地是否属于原生列表、迁移步数、国家与省份迁移修正，以及当前 ADM、DIP、MIL 资源，返回三项费用和逐项资源资格。已核验的正例为列表成员目的地 `[0, 0, 50]`、全部 power cost 修正为 `-5%` 时 MIL 费用 `48`、列表外且 `migration_steps=2` 时 MIL 费用 `150`，资源为 MIL `49` 时不合格、为 `50` 时合格。费用计算使用千分位固定点并对正的小数 power 向上取整；该接口只计算费用和资格，不提交迁移命令、不扣除资源，也不推断完整命令门槛、动态回调或迁移后的状态写入。

`calculate_vanilla_native_migration_gate` 组合调用方显式提供的费用与资格条件，返回 `resolved_gate`（`Some(true)`、`Some(false)` 或未知 `None`），并分别列出 `unresolved` 和 `rejected` 原因。这是调用方已解析条件的合取报告，不自行验证完整原版命令资格链。列表成员关系未知时费用及费用资格均为 `None`，不会使用费用输入中的占位成员值作出拒绝；明确失败的独立条件仍可返回拒绝。EU4 适配方法验证目标省份、核心国家实体及其当前首都关系，补齐省略的显式目标并拒绝冲突。原生模式、所有权、控制权、部落归属、其他资格和有效修正仍由调用方提供；报告不提交命令或修改状态。


`VanillaWorld::query_native_migration` 接受国家标签、目标省份及调用方解析的费用上下文和资格条件，每次从当前世界读取 ADM/DIP/MIL 整数点数及实时首都。CLI 可用 `--native-migration-queries queries.json` 提供查询数组，查询在 `--load` 恢复和 `--days` 推进之后执行，输出为 `native_migration_queries`；本次查询不影响快照，也不作为后续执行的授权或缓存许可。下面是自制的显式输入示例：

```json
[{
  "country": "AAA",
  "destination": 1,
  "pricing": {
    "define_adm_power_milli": 0,
    "define_dip_power_milli": 0,
    "define_mil_power_milli": 50000,
    "migration_steps": 2,
    "destination_in_native_membership": false,
    "all_power_cost_modifier_milli": 0,
    "country_migration_cost_modifier_milli": 0,
    "province_migration_cost_modifier_milli": 0
  },
  "native_membership_known": true,
  "native_mode_eligible": true,
  "owner_eligible": true,
  "controller_eligible": true,
  "tribal_owner_eligible": true,
  "additional_gate_eligible": true
}]
```

报告的 `available_power` 保留当前点数，未知值为 `null`。任何点数未知、为当前计算器尚不支持的负值或超出其整数范围时，`evaluation` 为 `null` 并在 `unresolved` 列出原因，不补零、不夹值或转换成费用拒绝；国家或目标省份不存在则报错。查询输入不接收资源快照或首都字段，因此相同请求在 MIL 从 `150` 变为 `149` 后会重新读取资源，费用为 `150` 时分别得到通过与拒绝。只有资源和调用方条件满足计算器要求时才返回 `evaluation`；费用 define、有效修正、成员关系、迁移步数和其余资格依然需要调用方解析，不从首都或归属猜测。


如需使用当前加载内容中的费用，可改用 `--native-migration-content-queries queries.json` 或 `VanillaWorld::query_native_migration_from_content`。请求结构与上例一致，但必须从 `pricing` 移除三个 `define_*_power_milli` 字段；该入口从已求值的 `NCountry.TRIBE_MIGRATION_ADM_COST`、`TRIBE_MIGRATION_DIP_COST`、`TRIBE_MIGRATION_MIL_COST` 取得费用，输出放在 `native_migration_content_queries`。`report.setup.native_migration_cost_defines_milli` 列出已读取的千分位值，`definition_sources` 的 `native_migration_cost:NAME` 项列出有效定义来源，包含 Mod 覆盖。定义必须非负、在既有 `Defines` 百万分位精度边界上能精确转换为千分位，且结果在有符号 32 位范围内；无效声明记录在 `native_migration_cost_define_errors`，并使内容费用查询报错；普通加载与显式费用查询不因此失败。缺项则返回 `evaluation: null` 和 `native_migration_cost:missing_define:NAME`，不把缺项当作零费用。

三项费用只供当前内容查询使用，不写入世界快照；恢复匹配的世界快照后，内容查询使用本次加载的有效定义。其余迁移修正、成员关系、步数和资格仍须显式提供。既有 `--native-migration-queries` 保持消费完整显式费用输入，不会被内容定义覆盖；两个入口都在推进之后只读评估，不预留资源或授权未来执行。


国家 `native` 块中的原始 `provinces` 列表与 `migration_steps` 可通过独立导入组件持久保存。CLI 同时传入 `--eu4-save save.eu4 --import-native-migration-state` 时启用该读取；也可用 `--native-migration-state states.json` 提供按国家标签索引的独立状态表，例如 `{"AAA":{"provinces":[2],"migration_steps":2}}`。两种输入互斥，在 `--load` 恢复前绑定，恢复快照后以快照状态为准。普通 `--eu4-save` 和既有外部状态包格式保持原约定，`--export-external-state` 不包含该独立组件；保存此状态使用 Augustus `--save`/`--load`。

原始块缺失时不声明该国家状态，块存在但字段缺失时对应值为未知；`provinces={}` 是明确空列表，`migration_steps=0` 是明确零。列表保留顺序，重复成员、无效整数、重复字段及无效结构会拒绝导入。读取后按目标世界过滤未知国家；如果名单包含目标世界中不存在的省份，整份成员列表保持未知，保留已知步数，不通过删除单个成员制造非成员结论。显式状态文件和运行时 `set_country_native_migration` 则严格拒绝未知国家或省份，整批验证后更新。成员关系独立于所有权、控制权及 `tribal_owner`，不会据这些字段重建列表。该运行状态随核心快照保存，不参与不可变内容身份计算；旧快照缺少组件时保持未知，不推断为已知空列表。

`--native-migration-state-queries queries.json` 或 `query_native_migration_from_state` 可进一步自动读取当前成员关系和步数，费用使用已加载内容 define，点数及首都使用当前世界。请求不再接收 `native_membership_known`，`pricing` 仅提供三个有效修正：

```json
[{
  "country":"AAA", "destination":1,
  "pricing":{
    "all_power_cost_modifier_milli":0,
    "country_migration_cost_modifier_milli":0,
    "province_migration_cost_modifier_milli":0
  },
  "native_mode_eligible":true, "owner_eligible":true,
  "controller_eligible":true, "tribal_owner_eligible":true,
  "additional_gate_eligible":true
}]
```

输出 `native_migration_state_queries` 的每项包含 `context` 与 `query`：前者列出当前原始名单、步数和目标成员关系；后者沿用实时费用查询报告。列表或步数未知时费用评估保持 `null` 并列出原因。查询不会修改状态、消耗随机流或执行迁移；模式、有效修正及其他资格仍须调用方解析。迁移后的名单维护、步数更新和完整命令生命周期尚未接通。

执行阶段可以通过 `VanillaWorld::execute_native_migration_from_state` 使用同一份当前状态查询结果。该入口会再次读取当前 ADM/DIP/MIL、当前 native 名单、步数和已加载费用定义；只有 gate 明确为 `true` 时才进入 core 事务。事务在暂存世界中预先检查三项 power 足够、名单和旧步数已知且仍等于调用方提供的 `expected_migration_steps`，然后扣除调用方已经解析的三项费用，并写入调用方明确提供的 `next_migration_steps`；成功后整体提交。gate 的三态保持区分：`None` 返回 `unresolved`，`Some(false)` 返回 `rejected`，`Some(true)` 才执行事务；前两者都不改变快照。执行阶段资源不足或旧步数复检失败也返回 `rejected` 且不改变快照。core 不自动推导 `steps+1` 或清零，因为已核验样本分别出现 `2→3` 和 `2→0`。当前执行阶段没有修改首都、所有权、controller、`tribal_owner` 或 native 省份名单；这些原版效果仍需后续模型和证据，不能把本入口称作完整迁徙命令生命周期。

`VanillaNativeMigrationExecutionRequest` 与 `VanillaNativeMigrationStateQuery` 都是严格的 JSON 请求契约：保留显式 `null` eligibility，`pricing` 与步数字段经过 roundtrip 后不变；顶层或嵌套 `pricing` 中出现未知字段会直接拒绝。执行请求中的 `expected_migration_steps` 和 `next_migration_steps` 始终由调用方提供，不能通过省略字段让适配层推断迁移步数。

`VanillaNativeMigrationTransferRequest` 使用相同的严格 JSON 边界：`expected_current_capital`、`expected_migration_steps` 和 `next_migration_steps` 经过 roundtrip 后保持不变，顶层或嵌套 `pricing` 的未知字段直接拒绝。transfer adapter 仍要求调用方显式提供这些前置值，不从当前世界状态猜测迁移目标或步数。

core 还提供 `World::clear_owner` 和 `InitialWorld::set_province_owner`，用于已经被调用方确认的省份所有权边界。清除 owner 会把关系记录为已知缺失并维护 OwnershipIndex；重新设置 owner 会移除该缺失标记。InitialWorld 入口在暂存副本中同时更新 core 所有权图和 EU4 省份投影，失败时保持原状态。它不推断资本、controller 或 `tribal_owner`，也不会把单独的 owner edge 更新描述成完整领土转移；native 迁徙仍需后续专用 transfer transaction。

core 的 `InitialWorld::execute_vanilla_native_migration_transfer` 进一步把已确认的目标省份、当前首都、三项 power 费用和显式步数更新放入同一个 staged transaction。它要求目标 owner 已明确为 known-absent，检查当前首都和旧步数没有变化，然后按“目标 owner → 新 capital → 旧 capital known-absent → controller/previous_controller → power/steps”一次提交；目标是未知 owner、第三方 capital、同一省份、owner 投影不一致、已知 controller 投影或任一前置值过期时整笔拒绝。`next_migration_steps` 始终由调用方显式提供，既支持 `2→3` 也支持已核验的 `2→0`；事务保留 raw native province list，不根据目标或步骤自动增删；已有 `tribal_owner` 关系也保持不变。

core 现在还保存省份 controller 的三态关系：缺少 map key 表示未知，`Some(None)` 表示已知无 controller，`Some(Some(country))` 表示已知 controller。`InitialWorld::set_province_controller` 在暂存副本中同步关系和投影；旧快照缺少该字段时仍恢复为未知。native migration transfer 会在同一 staged transaction 中把旧首都 controller 记为 known absent，并把目标省份 controller 记为 source country。

core 也独立保存省份 `previous_controller` 与 `tribal_owner`，国家 `original_capital`，以及国家 dated capital history 的三态关系：缺少 map key 表示未知，`Some(None)` 表示明确知道没有关系，`Some(Some(entity))` 表示已知实体。`previous_controller` 不写入 `ProvinceState`，`original_capital` 不复用当前 `Entity.capital` 或内容 `declared_capital`，dated history 使用可序列化的事件序列保存。上述字段不会从 owner、controller 或 native list 反推；通用 controller 更新不会自动产生历史记录。native migration transfer 只有在旧首都当前 controller 已明确存在时才记录其为 previous controller，旧 controller 未知时保持历史未知，目标省份已有的 previous controller 保持不变；迁徙事务保持旧首都和目标省份已有的 `tribal_owner` 与 `original_capital` 状态不变。五张关系表都使用 `serde(default)` 兼容旧快照，并从 content identity 中排除；`tribal_owner` 与 raw native 名单是独立关系，不能互相推断。

EU4 适配层的 `VanillaWorld::execute_native_migration_transfer_from_state` 先重新查询当前 native 名单、步数、首都和 power，再把已加载内容定义与调用方 eligibility 交给该 core transaction；gate 未解析返回 `Unresolved`，明确拒绝或 core 前置条件失败返回 `Rejected`，失败时保持快照不变。它要求目标 owner 已经由 bootstrap 或调用方标记为 known-absent，并且只支持 old/destination current controller projection 未解析的状态；core transfer 会同步 current controller 与已知的 previous controller 历史，同时保留已有的 tribal-owner 与 original-capital 关系。这条 API 仍不代表原版 save writer 或完整命令生命周期。

如果另一套游戏实例能够提供多类状态，可以使用 `VanillaExternalState` 一次性注入国家指标、每项指标的增长上限、国库与人力、当前国家焦点、已确认的阶层特权和忠诚度目标、当前省份原始自治度、已经翻译好的国家级月度现金流、已经核实的月度君主点数收入、开局陆军和舰船，以及按 EU4 省份编号给出的生产输入和税收输入。`country_national_focus` 省略国家表示未知，`{"skill": null}` 表示明确清除焦点；存档读取器会把未压缩 `EU4txt` 中的当前焦点翻译到这个字段。存档中的 `current_power_projection` 会进入国家指标 `power_projection`，单位为千分位；缺少字段不补零。`country_metric_ceilings` 只写入提供者已经确认的动态增长上限；省略的国家或指标继续保持未知，Augustus 不会把 `POWER_MAX` 或其他基础定义自动当成每个国家的最终上限。`country_monthly_power_income` 按国家标签和 `adm_power`、`dip_power`、`mil_power` 绑定已经核实的月度收入；显式的 `null` 保持该项收入未知，未列出的国家或指标继续使用本地内容公式。它只替换收入公式，不替代点数库存和增长上限；战役运行中可以调用 `VanillaWorld::set_country_monthly_power_income` 刷新同一边界。`estate_privileges` 使用国家标签、阶层标识和特权集合绑定当前已授予特权；`Some([])` 表示明确没有特权，省略或 `null` 仍表示未知。`province_raw_autonomy_milli` 使用千分位比例绑定当前行政状态，之后的月度自治规则会从这份状态继续运行；`country_annual_tax_milli` 替换国家级年税来源；`province_annual_tax_milli` 按省份替换 `tax_after_autonomy` 的年化值，显式的 `null` 表示该省份仍未知，未列出的省份继续使用本地内容计算。税收输入使用年化千分位整数，月结时统一换算为月收入。月度现金流使用千分位整数，正数表示收入，负数表示支出；Augustus 会在税收和普通生产之后应用它，并保留每个国家的结算记录。`production_inputs` 接收已经算好的最终生产基础值；`production_value_inputs` 则接收货物数量、价值阶段修正和月结修正，由兼容层从当前 EU4 内容读取商品基础价格后生成最终输入。两种生产格式不能同时声明同一省份。开局单位在启动事件之前绑定，因此启动脚本可以读取这些单位；单位来源仍由提供者负责翻译，Augustus 不从其他游戏复制脚本或规则。该对象在启动事件之前绑定，核心只接收已经翻译好的数值；缺少的国家、指标或省份仍保持未知，错误的标签、省份编号或无港口舰船位置会拒绝整个输入。命令行可直接读取 JSON 状态包：

```json
{
  "country_metric_values": {"HAB": {"prestige": 26942, "power_projection": 50000}},
  "country_national_focus": {
    "HAB": {"skill": "Administration"}
  },
  "advisors": {
    "HAB": [
      {"type_name": "treasurer", "skill": 1, "id": 2018}
    ]
  },
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
  "naval_fleets": [
    {"owner": "HAB", "location": 135, "name": "Channel Patrol", "ship_types": ["light_ship"]}
  ]
}
```

`naval_fleets` 保留舰队及舰船编组。若外部来源只能提供散装舰船，可以继续使用 `naval_units`；每艘船会作为单舰队导入。两种字段同时存在时，以 `naval_fleets` 为准。

```sh
cargo run -p augustus-run --bin augustus-world --locked -- \
  --game-root "/path/to/Europa Universalis IV" \
  --external-state "/path/to/external-state.json" \
  --country HAB --days 30
```

状态包是运行时输入，不会把另一套游戏的脚本或规则复制进 Augustus；提供者负责读取、校验并翻译来源游戏的状态。

战役运行中可以调用 `VanillaWorld::set_country_monthly_cashflows` 更新已经安装的现金流表。它只替换下一次月结使用的输入，不重置税收、生产或现金流的结算次数。现金流来源未知时，应传入 `null`；核心会保留未知状态，不把它当成零。

可以通过 `VanillaWorld::set_country_monthly_cashflow_breakdowns` 提供分类账项。兼容层提供完整节点行集合、已解析的资格和收入上下文后，可调用 `calculate_vanilla_trade_node_income_pass` 将节点份额转换为 `trade_income_pass`；行集合不完整或任一候选行未决时，整国贸易收入保持未知，不采用部分合计。核心负责计算记录收入总额，并将结果用于净现金流和贸易账本；也可直接提供已核实的 `trade_milli`。同时提供两者时必须一致。外部状态中的 `gold_production_inputs` 可按省份提供已解析的金矿输入；输入随存档保存，核心在每次月结时按当时的省份所有权重新汇总并更新 `gold_milli`。运行中可调用 `VanillaWorld::set_vanilla_gold_production_inputs` 刷新输入。有金矿省份缺少输入时黄金收入保持未知。贸易、黄金和支出都已知时，核心会据当期金矿所有权重算净现金流；分类不全时保留外部提供的净现金流。黄金是收入，支出使用正数；三项齐全时核心会校验分类账项与净现金流一致。缺失的分类继续保留为未知。

兼容提供者可以调用 `VanillaWorld::set_vanilla_fort_maintenance_expense_categories` 计算并替换已提供省份行的要塞维护分类明细。活动要塞资格及各省份修正仍由提供者解析；该接口不会重算总支出或净现金流。

`VanillaWorld::set_vanilla_fort_maintenance_expense_categories_from_buildings` 会按当前所有权和已载入建筑定义自动识别带正 `fort_level` 的要塞省份，并使用已核验的 EU4 1.37.5 固定基础费与货币换算。国家、省份及关系修正和特殊减半条件仍须显式提供；缺少建筑状态、要塞等级、多个要塞建筑或候选行输入不全时会报错。它同样只更新类别 8 明细，不重算总支出或净现金流。

国家属性查询中的 `fort_maintenance_modifier` 与省份属性中的 `local_fort_maintenance_modifier` 会列出已绑定来源的小计；只要仍有未绑定来源，最终值就保持未知。这些小计不会自动代替维护费结算输入。

生产执行器安装后，兼容提供者可以调用 `VanillaWorld::set_vanilla_production_inputs` 刷新下一次月结使用的省份输入。刷新不会重置已经完成的月结次数、账本或存档状态；输入仍按 EU4 省份编号绑定，不能借此创建不存在的省份。

如果提供者保留的是货物数量和生产阶段修正，可以改用 `VanillaWorld::set_vanilla_production_value_inputs`。它会重新读取当前 EU4 内容中的商品基础价格，再刷新下一次月结输入；已有月结次数和账本保持不变。没有商品价格的省份会保留为未知。

战役已经载入后，兼容提供者还可以调用 `VanillaWorld::set_estate_loyalty_targets` 更新已有阶层的目标。这个入口只修改现存国家和阶层，不会凭空创建阶层；未知标签或阶层会拒绝整个更新。

Godot 页面提供状态包文件选择器，也接受 `--external-state "/path/to/external-state.json"`。状态包可以和玩家标签、行政基线存档一起载入；页面会在启动事件前把它交给兼容层，未提供的字段继续保持未知。命令行还可以把状态包叠加到 `--eu4-save` 的翻译结果上；两者提供的同名字段以状态包为准。

入口读取 `common/cb_types`，在 `casus_belli` 报告中列出定义来源、显式授予类型数量及尚未编译的保留条件。受支持的启动授予使用核心的有向记录；启动初始化未完成时清除部分写入并将库存标为未知。其他书签的历史授予尚未导入，同样不能判成已知没有记录。自动生成、宣战权限和战争目标尚未运行。自制样本及接口见 [宣战理由记录演示](../examples/justifications/README.md)。

开局条件和事件中的 `exists` 复用核心领土关系。海域等非陆地省份，以及已完整读取历史且未指定所有者的省份，可明确标为无主；陆地历史缺失或包含尚未处理的嵌套所有权声明时保留未知，不能据此断言某国不存在。核心宣称不赋予领土；标签查询从已登记的国家实体读取当前关系。支持范围见 [国家领土条件演示](../examples/country-existence/README.md)。

可以使用本地原版开局存档核对行政状态。在 EU4 1.37.5 中启用全部玩法 DLC、禁用所有 Mod，新建非铁人 `1444.11.11` 游戏，保持暂停，保存时取消“压缩”。然后运行：

```sh
cargo run -p augustus-run --bin augustus-world --locked -- --game-root "/path/to/Europa Universalis IV" --compare-administration-save "/path/to/baseline.eu4"
```

`administration_comparison` 分别统计所有者、核心及类型、州与领地关系、原始自治度的 `matched`、`different` 和 `unknown`，并列出已知值的差异。`unknown` 不计为通过；没有差异也不代表尚未实现的自治度或其他游戏机制已经一致。工具要求存档记录版本为 1.37.5.0、启用 22 个玩法 DLC、无模组，开局日期和当前日期均为 1444.11.11，`campaign_length = 0`。条件不符、相关字段重复或引用结构无效时会报错。存档中的省份自治缺省按其格式表示零，这不改变 Augustus 中尚未确定的初值。

该入口只读取未压缩的 `EU4txt` 基线存档用于对照，不导入状态或修改文件；ZIP 和令牌化二进制存档不受此对照入口支持。它检查存档记录的条件，不能代替对实验程序和安装内容来源的确认。

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

未压缩 EU4 存档中的省份 `is_city=yes/no` 会通过 `VanillaExternalState.province_is_city` 绑定到省份状态，供税收属性读取城市修正；字段缺失时保持未知，不补默认值。



`augustus-world` 通过 `--native-migration-transfers FILE` 接收请求数组，在 restore 后、`--days` 推进前按数组顺序调用 native migration transfer adapter；输出放在 `native_migration_transfers`，`--save` 会保存执行后的 Augustus 快照。请求仍需提供 caller-resolved gate 输入、当前首都、旧步骤和 next steps；CLI 执行结果不等同于原版 EU4 save writer。请求数组中的后续请求读取前一个请求提交后的实时状态；`rejected` 和 `unresolved` 报告不写入 core transaction，因此保存结果只保留此前成功请求的状态。CLI 还验证了显式 native state 先写入 snapshot、随后省略该输入并 restore 后再执行 transfer 的路径，证明 snapshot restore 在 transfer request 之前生效。

core 还提供纯计算的 `calculate_vanilla_native_migration_score` 与 `select_vanilla_native_migration_candidate`。它们只接收调用方已经解析的贸易因子、`a0d180` 结果、荒废度、native membership、migration steps 和归一化距离惩罚；非成员候选按 `steps * 10000` 与最小距离惩罚扣分，成员候选保留基础分，winner 只有在分数严格更高时才替换。该模块不从地图、随机源或 EU4 脚本推导输入，也不接入命令队列、扣款或自动迁徙。

`CountryView.capital_history` 在 Rust 内部继续使用 `WorldDate`，JSON view 序列化时将日期键编码为 `YYYY.MM.DD` 字符串，避免 `serde_json` 对非字符串 map key 的拒绝。CLI 组合 roundtrip 已覆盖当前 capital、original capital、dated history、previous controller 与 tribal owner 的保存和恢复。

`augustus-world` 还提供只读的 `--native-migration-score-queries FILE`。输入是 caller-resolved `VanillaNativeMigrationScoreCandidateInput` 数组，输出每个候选的分数和严格大于规则选出的 winner；它不读取世界状态来补全 trade factor、`a0d180`、距离或随机源，也不修改 snapshot 或提交迁徙命令。

如果所有候选都不可解析或明确不合格，单项 `score` 输出为 `null`，整体 `winner` 也输出为 `null`；如果输入数组为空，`candidates` 保持为空数组、`winner` 仍为 `null`。这些只读查询都不会改变保存的世界快照。

score query 的 JSON 解析和计算在 `--save` 文件创建之前完成。未知字段、负距离惩罚或其他计算错误会使命令失败且不留下 snapshot 文件。

native migration transfer CLI 也在保存前完成请求解析和 adapter 校验。未知 transfer 字段、未知国家或未知目的地会失败且不创建 snapshot；已提交的前序请求只在整个请求调用成功后才会出现在保存结果中。

score core 输入和候选结构使用 `serde(deny_unknown_fields)`；JSON roundtrip 会保留 `migration_steps = null` 与归一化距离向量，未知嵌套字段直接拒绝。CLI 的 score query 解析复用这份契约。

EU4 country history 中 `capital=0` 会保留为显式无值；`CountryView.capital_history` JSON 输出为日期键对应的 `null`，并在 Augustus snapshot reload 后保持该三态信息。

EU4 country 中的 `original_capital=0` 也保留为显式无值；`CountryView.original_capital` JSON 输出为 `null`，并在 Augustus snapshot reload 后保持该三态信息。它与当前 `capital`、dated capital history 和内容声明的首都分别建模，不会被其他首都字段补回。

// SPDX-License-Identifier: GPL-3.0-only

//! 给 AI 调用的 stdio MCP。一个进程保存一份开局，和正在运行的 Godot 窗口互不改写。

use std::{path::PathBuf, sync::Arc};

use augustus_core::Simulation;
use augustus_eu4::world::OpeningWorld;
use rmcp::{
    ErrorData as McpError, ServiceExt,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    schemars, tool, tool_router,
    transport::stdio,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::{debug, screenshot};

struct RunningGame {
    opening: OpeningWorld,
    simulation: Simulation,
    player: String,
    debug_dir: PathBuf,
}

impl RunningGame {
    fn open(game_dir: PathBuf, country: String, debug_dir: PathBuf) -> Result<Self, String> {
        let opening = OpeningWorld::load(&game_dir).map_err(|error| error.to_string())?;
        let summary = opening
            .country_summary(&country)
            .ok_or("请选择有效的国家")?;
        if summary.provinces == 0 {
            return Err("只能选择开局时拥有领土的国家".to_owned());
        }
        std::fs::create_dir_all(&debug_dir)
            .map_err(|error| format!("无法创建调试目录：{error}"))?;
        let simulation = Simulation::new(opening.start_date());
        Ok(Self {
            opening,
            simulation,
            player: country,
            debug_dir,
        })
    }
}

#[derive(Clone)]
struct AugustusDebug {
    session: Arc<Mutex<Option<RunningGame>>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct StartArgs {
    /// EU4 安装目录。
    game_dir: String,
    /// 玩家国家标签。
    country: String,
    /// 与游戏启动参数 `--debug-dir` 相同的目录。截图请求写在这里。
    debug_dir: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CountryArgs {
    /// 国家标签。省略时查询玩家国家。
    tag: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ProvinceArgs {
    /// 省份编号。
    id: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ArmyArgs {
    /// 军队编号。
    id: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ArmiesArgs {
    /// 只返回该国军队。省略时返回全部军队。
    tag: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct BattleArgs {
    /// 战斗编号。
    id: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct PeaceArgs {
    /// 对方国家标签。
    enemy: String,
    /// 提出方。省略时使用玩家国家。
    country: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeclareWarArgs {
    /// 被宣战的国家标签。
    defender: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MoveArmyArgs {
    /// 军队编号。
    id: u32,
    /// 目标省份。
    destination: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ArmyIdArgs {
    /// 军队编号。
    id: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RecruitArgs {
    /// 省份编号。
    province: u32,
    /// `infantry` 或 `cavalry`。
    kind: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CancelRecruitmentArgs {
    /// 省份编号。
    province: u32,
    /// 招募订单编号。
    order_id: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MakePeaceArgs {
    /// 对方国家标签。
    enemy: String,
    /// 割让的省份。省略或空列表表示白和平。
    #[serde(default)]
    provinces: Vec<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AdvanceArgs {
    /// 推进的游戏日数。不受暂停和倍速影响。
    days: u32,
}

#[tool_router(server_handler)]
impl AugustusDebug {
    fn new() -> Self {
        Self {
            session: Arc::new(Mutex::new(None)),
        }
    }

    /// 加载 EU4 开局并选定玩家国家。这个会话独立于正在运行的游戏窗口。再次调用会替换当前会话。
    #[tool(
        description = "加载 EU4 开局并选定玩家国家。会话独立于正在运行的游戏窗口。再次调用会替换当前会话。"
    )]
    async fn start(
        &self,
        Parameters(args): Parameters<StartArgs>,
    ) -> Result<CallToolResult, McpError> {
        let game = RunningGame::open(
            PathBuf::from(args.game_dir),
            args.country,
            PathBuf::from(args.debug_dir),
        );
        match game {
            Ok(game) => {
                let report = debug::clock_report(&game.simulation);
                let mut session = self.session.lock().await;
                let player = game.player.clone();
                *session = Some(game);
                json_result(Ok(json!({
                    "ok": true,
                    "player": player,
                    "clock": report,
                })))
            }
            Err(error) => tool_error(error),
        }
    }

    #[tool(description = "读取会话中的日期、暂停和倍速。")]
    async fn clock(&self) -> Result<CallToolResult, McpError> {
        let session = self.session.lock().await;
        let Some(game) = session.as_ref() else {
            return tool_error("请先开始游戏");
        };
        json_result(Ok(debug::clock_report(&game.simulation)))
    }

    #[tool(description = "查询一个国家的财政、人力、陆军上限、战争和停战。")]
    async fn country(
        &self,
        Parameters(args): Parameters<CountryArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.query(|game| {
            let tag = args.tag.as_deref().unwrap_or(game.player.as_str());
            let name = game.opening.display_name(tag);
            debug::country_report(game.opening.state(), tag, &name)
        })
        .await
    }

    #[tool(description = "查询一个省份的归属、发展度、建筑、占领、招募和战斗状态。")]
    async fn province(
        &self,
        Parameters(args): Parameters<ProvinceArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.query(|game| {
            let name = game.opening.province_name(args.id);
            let date = game.simulation.clock().date();
            debug::province_report(game.opening.state(), args.id, &name, date)
        })
        .await
    }

    #[tool(description = "查询一支军队的位置、兵力、士气和行军。")]
    async fn army(
        &self,
        Parameters(args): Parameters<ArmyArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.query(|game| {
            debug::army_report(
                game.opening.state(),
                args.id,
                game.simulation.clock().date(),
            )
        })
        .await
    }

    #[tool(description = "列出军队。可以只看一个国家。")]
    async fn armies(
        &self,
        Parameters(args): Parameters<ArmiesArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.query(|game| {
            debug::armies_report(
                game.opening.state(),
                args.tag.as_deref(),
                game.simulation.clock().date(),
            )
        })
        .await
    }

    #[tool(description = "查询一场进行中的战斗。")]
    async fn battle(
        &self,
        Parameters(args): Parameters<BattleArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.query(|game| debug::battle_report(game.opening.state(), args.id))
            .await
    }

    #[tool(description = "列出进行中的战斗。")]
    async fn battles(&self) -> Result<CallToolResult, McpError> {
        self.query(|game| Ok(debug::battles_report(game.opening.state())))
            .await
    }

    #[tool(description = "查询和谈分数、白和平和可割让省份。不改变战争。")]
    async fn peace(
        &self,
        Parameters(args): Parameters<PeaceArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.query(|game| {
            let country = args.country.as_deref().unwrap_or(game.player.as_str());
            debug::peace_report(
                game.opening.state(),
                country,
                &args.enemy,
                game.simulation.clock().date(),
            )
        })
        .await
    }

    #[tool(description = "以玩家国家宣战。")]
    async fn declare_war(
        &self,
        Parameters(args): Parameters<DeclareWarArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.play(|game| {
            let date = game.simulation.clock().date();
            let player = game.player.clone();
            debug::declare_war(game.opening.state_mut(), &player, &args.defender, date)
        })
        .await
    }

    #[tool(description = "命令玩家国家的军队行军。")]
    async fn move_army(
        &self,
        Parameters(args): Parameters<MoveArmyArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.play(|game| {
            let date = game.simulation.clock().date();
            let player = game.player.clone();
            debug::move_army(
                game.opening.state_mut(),
                &player,
                args.id,
                args.destination,
                date,
            )
        })
        .await
    }

    #[tool(description = "停止玩家国家军队的后续行军。")]
    async fn stop_army(
        &self,
        Parameters(args): Parameters<ArmyIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.play(|game| {
            let player = game.player.clone();
            debug::stop_army(game.opening.state_mut(), &player, args.id)
        })
        .await
    }

    #[tool(description = "命令玩家国家正在交战的军队撤退。")]
    async fn retreat_army(
        &self,
        Parameters(args): Parameters<ArmyIdArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.play(|game| {
            let date = game.simulation.clock().date();
            let player = game.player.clone();
            debug::retreat_army(game.opening.state_mut(), &player, args.id, date)
        })
        .await
    }

    #[tool(description = "在玩家国家控制的省份招募。kind 为 infantry 或 cavalry。")]
    async fn recruit(
        &self,
        Parameters(args): Parameters<RecruitArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.play(|game| {
            let date = game.simulation.clock().date();
            let player = game.player.clone();
            debug::recruit(
                game.opening.state_mut(),
                &player,
                args.province,
                &args.kind,
                date,
            )
        })
        .await
    }

    #[tool(description = "取消玩家国家的一笔招募订单。")]
    async fn cancel_recruitment(
        &self,
        Parameters(args): Parameters<CancelRecruitmentArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.play(|game| {
            let date = game.simulation.clock().date();
            let player = game.player.clone();
            debug::cancel_recruitment(
                game.opening.state_mut(),
                &player,
                args.province,
                args.order_id,
                date,
            )
        })
        .await
    }

    #[tool(description = "以玩家国家缔结白和平或割让。provinces 为空表示白和平。")]
    async fn make_peace(
        &self,
        Parameters(args): Parameters<MakePeaceArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.play(|game| {
            let date = game.simulation.clock().date();
            let player = game.player.clone();
            debug::make_peace(
                game.opening.state_mut(),
                &player,
                &args.enemy,
                &args.provinces,
                date,
            )
        })
        .await
    }

    #[tool(
        description = "按游戏日推进会话。不受暂停和倍速影响，也不改变暂停。结果包含这些日子里非玩家交战国的决定。"
    )]
    async fn advance(
        &self,
        Parameters(args): Parameters<AdvanceArgs>,
    ) -> Result<CallToolResult, McpError> {
        self.play(|game| {
            let player = game.player.clone();
            let step = debug::advance(
                &mut game.simulation,
                game.opening.state_mut(),
                args.days,
                &player,
            );
            Ok(json!({
                "days": step.days,
                "date": game.simulation.clock().date().to_string(),
                "paused": game.simulation.clock().is_paused(),
                "decisions": step.decisions,
            }))
        })
        .await
    }

    /// 向带有同一个 debug_dir 的游戏要当前窗口截图。游戏没开时返回“游戏未运行”，不代为启动。
    #[tool(
        description = "取得正在运行的游戏窗口截图。游戏须用同一个 --debug-dir 启动。没开时返回“游戏未运行”，不代为启动。画面来自游戏窗口，不是这个会话的推演。"
    )]
    async fn screenshot(&self) -> Result<CallToolResult, McpError> {
        let debug_dir = {
            let session = self.session.lock().await;
            let Some(game) = session.as_ref() else {
                return tool_error("请先开始游戏");
            };
            game.debug_dir.clone()
        };
        match screenshot::request_screenshot(&debug_dir) {
            Ok(bytes) => {
                let encoded =
                    base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
                Ok(CallToolResult::success(vec![
                    ContentBlock::text("已取得当前游戏窗口截图"),
                    ContentBlock::image(encoded, "image/png"),
                ]))
            }
            Err(error) => tool_error(error),
        }
    }

    async fn query(
        &self,
        action: impl FnOnce(&RunningGame) -> Result<Value, String>,
    ) -> Result<CallToolResult, McpError> {
        let session = self.session.lock().await;
        let Some(game) = session.as_ref() else {
            return tool_error("请先开始游戏");
        };
        json_result(action(game))
    }

    async fn play(
        &self,
        action: impl FnOnce(&mut RunningGame) -> Result<Value, String>,
    ) -> Result<CallToolResult, McpError> {
        let mut session = self.session.lock().await;
        let Some(game) = session.as_mut() else {
            return tool_error("请先开始游戏");
        };
        json_result(action(game))
    }
}

fn json_result(result: Result<Value, String>) -> Result<CallToolResult, McpError> {
    match result {
        Ok(value) => match ContentBlock::json(value) {
            Ok(content) => Ok(CallToolResult::success(vec![content])),
            Err(error) => tool_error(error.to_string()),
        },
        Err(error) => tool_error(error),
    }
}

fn tool_error(message: impl Into<String>) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::error(vec![ContentBlock::text(
        message.into(),
    )]))
}

pub fn serve() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let service = AugustusDebug::new()
            .serve(stdio())
            .await
            .map_err(|error| -> Box<dyn std::error::Error> { error.to_string().into() })?;
        service
            .waiting()
            .await
            .map_err(|error| -> Box<dyn std::error::Error> { error.to_string().into() })?;
        Ok(())
    })
}

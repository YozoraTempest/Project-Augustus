// SPDX-License-Identifier: GPL-3.0-only

//! 与 `godot/autoload/game_session.gd` 约定同一目录下的四个文件。
//!
//! 游戏定期改写 `heartbeat`。本进程写入 `request.json`，等游戏在画完当前帧后
//! 保存 `shot.png` 并写回 `result.json`。没有新的心跳就视为游戏没在运行。

use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime},
};

use serde::{Deserialize, Serialize};

const HEARTBEAT_FILE: &str = "heartbeat";
const REQUEST_FILE: &str = "request.json";
const RESULT_FILE: &str = "result.json";
const IMAGE_FILE: &str = "shot.png";
const ALIVE: Duration = Duration::from_secs(2);
const WAIT: Duration = Duration::from_secs(3);

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ShotRequest {
    id: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct ShotResult {
    id: String,
    error: String,
}

/// 向正在运行的游戏要一张当前窗口截图。游戏没开时返回「游戏未运行」。
pub fn request_screenshot(debug_dir: &Path) -> Result<Vec<u8>, String> {
    if !game_alive(debug_dir) {
        return Err("游戏未运行".to_owned());
    }
    let id = next_id();
    write_request(debug_dir, &id)?;
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if let Some(result) = read_result(debug_dir)
            && result.id == id
        {
            if result.error.is_empty() {
                return fs::read(debug_dir.join(IMAGE_FILE))
                    .map_err(|error| format!("无法读取截图：{error}"));
            }
            return Err(result.error);
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err("游戏未返回截图".to_owned())
}

fn game_alive(debug_dir: &Path) -> bool {
    let modified = fs::metadata(debug_dir.join(HEARTBEAT_FILE))
        .and_then(|metadata| metadata.modified())
        .ok();
    modified.is_some_and(|modified| {
        SystemTime::now()
            .duration_since(modified)
            .is_ok_and(|age| age <= ALIVE)
    })
}

fn write_request(debug_dir: &Path, id: &str) -> Result<(), String> {
    let bytes = serde_json::to_vec(&ShotRequest { id: id.to_owned() })
        .map_err(|error| error.to_string())?;
    let path = debug_dir.join(REQUEST_FILE);
    let temporary = debug_dir.join("request.json.tmp");
    fs::write(&temporary, bytes).map_err(|error| format!("无法写入截图请求：{error}"))?;
    let _ = fs::remove_file(&path);
    fs::rename(&temporary, &path).map_err(|error| format!("无法写入截图请求：{error}"))
}

fn read_result(debug_dir: &Path) -> Option<ShotResult> {
    let bytes = fs::read(debug_dir.join(RESULT_FILE)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn next_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{nanos}-{sequence}")
}

#[cfg(test)]
mod tests {
    use std::{
        fs, thread,
        time::{Duration, Instant, SystemTime},
    };

    use super::{
        HEARTBEAT_FILE, IMAGE_FILE, REQUEST_FILE, RESULT_FILE, ShotRequest, ShotResult,
        request_screenshot,
    };

    #[test]
    fn missing_or_stale_game_is_not_running() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(request_screenshot(dir.path()).unwrap_err(), "游戏未运行");

        let heartbeat = dir.path().join(HEARTBEAT_FILE);
        fs::write(&heartbeat, b"1").unwrap();
        let file = fs::File::options().write(true).open(&heartbeat).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(10))
            .unwrap();
        assert_eq!(request_screenshot(dir.path()).unwrap_err(), "游戏未运行");
    }

    #[test]
    fn running_game_returns_the_saved_png() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(HEARTBEAT_FILE), b"1").unwrap();
        let directory = dir.path().to_path_buf();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if let Ok(bytes) = fs::read(directory.join(REQUEST_FILE)) {
                    let request: ShotRequest = serde_json::from_slice(&bytes).unwrap();
                    fs::write(directory.join(IMAGE_FILE), b"png-bytes").unwrap();
                    let result = ShotResult {
                        id: request.id,
                        error: String::new(),
                    };
                    fs::write(
                        directory.join(RESULT_FILE),
                        serde_json::to_vec(&result).unwrap(),
                    )
                    .unwrap();
                    return;
                }
                if Instant::now() > deadline {
                    panic!("没有收到截图请求");
                }
                thread::sleep(Duration::from_millis(10));
            }
        });
        let bytes = request_screenshot(dir.path()).unwrap();
        worker.join().unwrap();
        assert_eq!(bytes, b"png-bytes");
    }
}

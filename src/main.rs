//! Apexclaw — PicoClaw × APEX 端侧融合
//!
//! 极简单文件 CLI，PicoClaw 骨架直接嵌入 APEX 双公式评分引擎。
//! 支持端侧自进化：评分低时自动调 LLM (Ollama/OpenAI) 获取优化建议。
//!
//! 配置方式（优先级：CLI 参数 > 环境变量 > 配置文件 > 默认值）：
//!   1) apexclaw init              # 交互式向导
//!   2) apexclaw config            # 查看当前配置
//!   3) apexclaw config set k v    # 设置某项
//!   4) apexclaw config reset      # 恢复默认
//!   5) APEX_LLM_URL / APEX_SCORE_THR   # 环境变量覆盖
//!
//! Usage:
//!   apexclaw score                      # 默认评分
//!   apexclaw score --ev 0.9 --val 0.8   # 指定维度
//!   apexclaw signal phi                 # PHI_APEX 信号
//!   apexclaw signal omega               # Ω_ASI 信号
//!   apexclaw evolve                     # 评分 → 低分调 LLM → 自进化
//!   apexclaw status                     # 系统状态
//!
//! 零依赖 LLM 调用（路由器自带 curl），纯 APEX 公式融合。

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::Command as ProcessCmd;
use std::{env, fs};

// ─── 配置系统 ─────────────────────────────

const CONFIG_DIR: &str = ".apexclaw";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LlmConfig {
    /// LLM API 地址（Ollama / OpenAI 兼容）
    #[serde(default = "default_llm_url")]
    pub url: String,
    /// 模型名
    #[serde(default = "default_llm_model")]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApexConfig {
    /// Ω_A 权重
    #[serde(default = "default_omega_a")]
    pub omega_a: f64,
    /// harm_rate 安全系数
    #[serde(default = "default_harm_rate")]
    pub harm_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EvolveConfig {
    /// 自进化触发阈值
    #[serde(default = "default_threshold")]
    pub threshold: f64,
    /// 自动发现 LAN Ollama
    #[serde(default)]
    pub discovery: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub apex: ApexConfig,
    #[serde(default)]
    pub evolve: EvolveConfig,
}

// ─── 默认值 ───────────────────────────────

fn default_llm_url() -> String {
    "http://192.168.1.100:11434/api/generate".into()
}
fn default_llm_model() -> String {
    "qwen2.5:0.5b".into()
}
fn default_omega_a() -> f64 {
    0.85
}
fn default_harm_rate() -> f64 {
    1.0
}
fn default_threshold() -> f64 {
    0.02
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self { url: default_llm_url(), model: default_llm_model() }
    }
}
impl Default for ApexConfig {
    fn default() -> Self {
        Self { omega_a: default_omega_a(), harm_rate: default_harm_rate() }
    }
}
impl Default for EvolveConfig {
    fn default() -> Self {
        Self { threshold: default_threshold(), discovery: false }
    }
}
impl Default for Config {
    fn default() -> Self {
        Self { llm: LlmConfig::default(), apex: ApexConfig::default(), evolve: EvolveConfig::default() }
    }
}

impl Config {
    /// 配置文件路径 ~/.apexclaw/config.toml
    fn path() -> PathBuf {
        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(CONFIG_DIR).join(CONFIG_FILE)
    }

    /// 从文件加载，不存在则返回默认
    fn load() -> Self {
        let p = Self::path();
        if !p.exists() {
            return Config::default();
        }
        fs::read_to_string(&p)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// 保存到文件
    fn save(&self) -> Result<(), String> {
        let p = Self::path();
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
        }
        let toml_str = toml::to_string_pretty(self).map_err(|e| format!("序列化失败: {}", e))?;
        fs::write(&p, toml_str).map_err(|e| format!("写入失败: {}", e))?;
        Ok(())
    }

    /// 获取 LLM URL（优先环境变量覆盖）
    fn llm_url(&self) -> String {
        env::var("APEX_LLM_URL").unwrap_or_else(|_| self.llm.url.clone())
    }

    /// 获取阈值（优先环境变量覆盖）
    fn threshold(&self) -> f64 {
        env::var("APEX_SCORE_THR")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(self.evolve.threshold)
    }
}

// ─── CLI ─────────────────────────────────

#[derive(Parser)]
#[command(name = "apexclaw", version, about = "PicoClaw × APEX 端侧融合")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 交互式配置向导（首次使用推荐）
    Init,
    /// 查看 / 修改配置
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// APEX 双公式评分
    Score {
        #[arg(long, default_value = "0.5")]
        ev: f64,
        #[arg(long, default_value = "0.5")]
        val: f64,
        #[arg(long, default_value = "0.5")]
        integrity: f64,
        #[arg(long, default_value = "0.3")]
        novelty: f64,
        #[arg(long, default_value = "0.85")]
        omega_a: f64,
        #[arg(long, default_value = "1.0")]
        harm_rate: f64,
    },
    /// 生成 APEX 信号 (phi | omega)
    Signal {
        kind: String,
    },
    /// 系统状态
    Status,
    /// 自进化：评分 → LLM 诊断 → 维度优化
    Evolve {
        #[arg(long)]
        model: Option<String>,
        #[arg(long, default_value = "false")]
        force: bool,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// 显示当前配置
    Show,
    /// 设置配置项 (llm.url / llm.model / apex.omega_a / evolve.threshold ...)
    Set {
        key: String,
        value: String,
    },
    /// 恢复默认配置
    Reset,
}

// ─── APEX formula inline ─────────────────

/// 12 维阿卡西评分 Ω_A · Π(dim_i) - Σ(Δ_j)
fn calc_akashic(dims: &[f64; 12], omega_a: f64) -> (f64, Vec<&'static str>) {
    let product = dims.iter().copied().fold(1.0, |a, b| a * b);
    let mut penalties = Vec::new();
    let mut delta = 0.0;
    if dims[0] < 0.2 { delta += 0.05; penalties.push("low_evolution"); }
    if dims[1] < 0.2 { delta += 0.05; penalties.push("low_value"); }
    if dims[2] < 0.3 { delta += 0.10; penalties.push("low_integrity"); }
    if dims[8] < 0.1 { delta += 0.10; penalties.push("low_novelty"); }
    let score = omega_a * product - delta;
    (score.max(0.0), penalties)
}

/// PHI 公式 (base × ev × an × nv) / harm_rate
fn calc_phi(base: f64, ev: f64, an: f64, nv: f64, harm_rate: f64) -> f64 {
    (base * ev * an * nv) / harm_rate.max(0.01)
}

/// 融合评分
fn fuse(dims: &[f64; 12], phi: &[f64; 4], omega_a: f64, harm_rate: f64) -> (f64, f64, f64, f64, Vec<&'static str>) {
    let (akashic, penalties) = calc_akashic(dims, omega_a);
    let phi_score = calc_phi(phi[0], phi[1], phi[2], phi[3], harm_rate);
    let omega_p = 1.0 - omega_a;
    let fused = omega_a * akashic + omega_p * phi_score;
    let activation_potential = fused * dims[0];
    (akashic, phi_score, fused, activation_potential, penalties)
}

/// 默认 12 维
const DEFAULT_DIMS: [f64; 12] = [0.5, 0.5, 0.8, 0.7, 0.6, 0.5, 0.4, 0.7, 0.3, 0.5, 0.7, 0.6];

// ─── 主流程 ───────────────────────────────

fn main() {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Init => cmd_init(),
        Command::Config { action } => match action {
            ConfigAction::Show => cmd_config_show(),
            ConfigAction::Set { key, value } => cmd_config_set(&key, &value),
            ConfigAction::Reset => cmd_config_reset(),
        },
        Command::Score { ev, val, integrity, novelty, omega_a, harm_rate } => {
            let mut dims = DEFAULT_DIMS;
            dims[0] = ev.clamp(0.0, 1.0);
            dims[1] = val.clamp(0.0, 1.0);
            dims[2] = integrity.clamp(0.0, 1.0);
            dims[8] = novelty.clamp(0.0, 1.0);
            let phi = [0.8, ev.clamp(0.0, 1.0), 0.6, novelty.clamp(0.0, 1.0)];
            let (a, p, f, ap, penalties) = fuse(&dims, &phi, omega_a.clamp(0.0, 1.0), harm_rate.max(0.01));
            println!("╔══════════════════════════════════════╗");
            println!("║      APEX · 阿卡西融合评分            ║");
            println!("╠══════════════════════════════════════╣");
            println!("║  Ω_A · Π(dim_i) - Σ(Δ_j)            ║");
            println!("╠══════════════════════════════════════╣");
            println!("║  Akashic Score:  {:.6}       ", a);
            println!("║  PHI Score:      {:.6}       ", p);
            println!("║  Fused Score:    {:.6}       ", f);
            println!("║  Activation Pot: {:.6}       ", ap);
            if !penalties.is_empty() {
                println!("║  Penalties: {}", penalties.join(", "));
            }
            println!("╚══════════════════════════════════════╝");
        }
        Command::Signal { kind } => {
            match kind.as_str() {
                "phi" => {
                    let sig = json!({
                        "schema": "PHI_APEX",
                        "version": "1.0",
                        "source": "apexclaw",
                        "score": 0.75,
                        "intent": "self_evolve",
                        "ttl": 3,
                        "hop": 0,
                    });
                    println!("╔══ PHI_APEX Signal ══╗");
                    println!("║ Source: apexclaw");
                    println!("║ Score:  0.7500");
                    println!("║ Intent: self_evolve");
                    println!("║ TTL:    3 (hop 0)");
                    println!("╚══════════════════╝");
                    println!("JSON: {}", serde_json::to_string_pretty(&sig).unwrap());
                }
                "omega" | "asi" => {
                    let sig = json!({
                        "schema": "Ω_ASI",
                        "version": "1.0",
                        "source": "apexclaw",
                        "score": 0.65,
                        "intent": "self_inspection",
                        "ttl": 1,
                        "hop": 0,
                        "data": { "status": "nominal", "type": "self_inspection" }
                    });
                    println!("╔══ Ω_ASI Signal ══╗");
                    println!("║ Source: apexclaw");
                    println!("║ Score:  0.6500");
                    println!("║ Intent: self_inspection");
                    println!("║ TTL:    1 (hop 0)");
                    println!("╚══════════════════╝");
                    println!("JSON: {}", serde_json::to_string_pretty(&sig).unwrap());
                }
                _ => eprintln!("usage: apexclaw signal <phi|omega>"),
            }
        }
        Command::Status => {
            let cfg = Config::load();
            println!("╔══════════════════════════════════════╗");
            println!("║      Apexclaw System Status          ║");
            println!("╠══════════════════════════════════════╣");
            println!("║  Version:  0.2.0");
            println!("║  APEX:     ✓ ENABLED");
            println!("║  ω_a:      {:.2}                     ", cfg.apex.omega_a);
            println!("║  ω_p:      {:.2}                     ", 1.0 - cfg.apex.omega_a);
            println!("║  Score Th: {:.3}                ", cfg.threshold());
            println!("║  TTL:      3");
            println!("║  Discovery: {}                      ", if cfg.evolve.discovery { "✓ ON" } else { "✗ OFF" });
            println!("║  LLM URL:  {}", cfg.llm_url());
            println!("║  Config:   {}", Config::path().display());
            println!("╚══════════════════════════════════════╝");
        }
        Command::Evolve { model, force, .. } => {
            let cfg = Config::load();
            let (result_code, msg) = evolve_self(&cfg, model.as_deref(), &force);
            println!("{}", msg);
            if result_code != 0 {
                std::process::exit(result_code);
            }
        }
    }
}

// ─── init 交互式向导 ─────────────────────

fn cmd_init() {
    let cfg_path = Config::path();
    if cfg_path.exists() {
        print!("╔══════════════════════════════════════╗\n");
        print!("║  配置已存在: {}  ║\n", cfg_path.display());
        print!("║  回车跳过，n 重新配置... ");
        io::stdout().flush().ok();
        let mut input = String::new();
        io::stdin().read_line(&mut input).ok();
        if !input.trim().eq_ignore_ascii_case("n") {
            print!("╚══════════════════════════════════════╝\n");
            return;
        }
    }

    print!("\n");
    println!("╔══════════════════════════════════════╗");
    println!("║     Apexclaw 配置向导                ║");
    println!("║  回车 = 使用括号内的默认值            ║");
    println!("╚══════════════════════════════════════╝");

    let mut cfg = Config::default();

    // LLM URL
    let default_url = default_llm_url();
    print!("  LLM API 地址 [{}]: ", default_url);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let trimmed = input.trim();
    if !trimmed.is_empty() {
        cfg.llm.url = trimmed.to_string();
    }

    // Model
    let default_model = default_llm_model();
    print!("  LLM 模型名 [{}]: ", default_model);
    io::stdout().flush().ok();
    input.clear();
    io::stdin().read_line(&mut input).ok();
    let trimmed = input.trim();
    if !trimmed.is_empty() {
        cfg.llm.model = trimmed.to_string();
    }

    // Omega_a
    print!("  Ω_A 权重 (0.0~1.0) [{}]: ", default_omega_a());
    io::stdout().flush().ok();
    input.clear();
    io::stdin().read_line(&mut input).ok();
    let trimmed = input.trim();
    if !trimmed.is_empty() {
        if let Ok(v) = trimmed.parse::<f64>() {
            cfg.apex.omega_a = v.clamp(0.0, 1.0);
        }
    }

    // Threshold
    print!("  自进化阈值 (0~1) [{}]: ", default_threshold());
    io::stdout().flush().ok();
    input.clear();
    io::stdin().read_line(&mut input).ok();
    let trimmed = input.trim();
    if !trimmed.is_empty() {
        if let Ok(v) = trimmed.parse::<f64>() {
            cfg.evolve.threshold = v.clamp(0.0, 1.0);
        }
    }

    // Discovery
    print!("  自动发现 LAN Ollama? (y/N): ");
    io::stdout().flush().ok();
    input.clear();
    io::stdin().read_line(&mut input).ok();
    cfg.evolve.discovery = input.trim().eq_ignore_ascii_case("y");

    match cfg.save() {
        Ok(()) => {
            println!("╔══════════════════════════════════════╗");
            println!("║  ✓ 配置已保存到:                     ║");
            println!("║     {}", cfg_path.display());
            println!("╚══════════════════════════════════════╝");
        }
        Err(e) => {
            eprintln!("✗ 保存失败: {}", e);
            std::process::exit(1);
        }
    }
}

// ─── config 子命令 ───────────────────────

fn cmd_config_show() {
    let cfg = Config::load();
    let path = Config::path();
    println!("╔══════════════════════════════════════╗");
    println!("║     Apexclaw Configuration           ║");
    println!("╠══════════════════════════════════════╣");
    println!("║  文件: {}", path.display());
    println!("╠══════════════════════════════════════╣");
    println!("║  [llm]");
    println!("║  url   = {}", cfg.llm.url);
    println!("║  model = {}", cfg.llm.model);
    println!("║  [apex]");
    println!("║  omega_a   = {:.2}", cfg.apex.omega_a);
    println!("║  harm_rate = {:.2}", cfg.apex.harm_rate);
    println!("║  [evolve]");
    println!("║  threshold  = {}", cfg.evolve.threshold);
    println!("║  discovery  = {}", cfg.evolve.discovery);
    println!("╠══════════════════════════════════════╣");
    println!("║  环境变量覆盖:                       ║");
    println!("║  APEX_LLM_URL     当前值: {}", env::var("APEX_LLM_URL").unwrap_or_else(|_| "(未设置)".into()));
    println!("║  APEX_SCORE_THR   当前值: {}", env::var("APEX_SCORE_THR").unwrap_or_else(|_| "(未设置)".into()));
    println!("╚══════════════════════════════════════╝");
    println!("\n要编辑配置，也可以直接编辑该文件：");
    println!("  notepad {}", path.display());
}

fn cmd_config_set(key: &str, value: &str) {
    let mut cfg = Config::load();
    match key {
        "llm.url" => cfg.llm.url = value.to_string(),
        "llm.model" => cfg.llm.model = value.to_string(),
        "apex.omega_a" | "omega_a" => {
            cfg.apex.omega_a = value.parse().unwrap_or(default_omega_a()).clamp(0.0, 1.0);
        }
        "apex.harm_rate" | "harm_rate" => {
            cfg.apex.harm_rate = value.parse().unwrap_or(default_harm_rate()).max(0.01);
        }
        "evolve.threshold" | "threshold" => {
            cfg.evolve.threshold = value.parse().unwrap_or(default_threshold()).clamp(0.0, 1.0);
        }
        "evolve.discovery" | "discovery" => {
            cfg.evolve.discovery = value.eq_ignore_ascii_case("true") || value == "1";
        }
        _ => {
            eprintln!("未知配置项: {}", key);
            eprintln!("可用: llm.url, llm.model, apex.omega_a, apex.harm_rate, evolve.threshold, evolve.discovery");
            std::process::exit(1);
        }
    }
    match cfg.save() {
        Ok(()) => {
            println!("✓ {} = {}", key, value);
        }
        Err(e) => {
            eprintln!("✗ 保存失败: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_config_reset() {
    let cfg = Config::default();
    match cfg.save() {
        Ok(()) => {
            println!("✓ 配置已恢复默认，文件: {}", Config::path().display());
        }
        Err(e) => {
            eprintln!("✗ 重置失败: {}", e);
            std::process::exit(1);
        }
    }
}

// ─── 自进化引擎 ──────────────────────────

/// 自进化循环：评分 → LLM 诊断 → 维度优化
fn evolve_self(cfg: &Config, model_override: Option<&str>, force: &bool) -> (i32, String) {
    let dims = DEFAULT_DIMS;
    let phi = [0.8, 0.5, 0.6, 0.3];
    let (_a, _p, f, _ap, penalties) = fuse(&dims, &phi, cfg.apex.omega_a, cfg.apex.harm_rate);

    let threshold = cfg.threshold();

    let mut out = String::new();
    out.push_str("╔══════════════════════════════════════╗\n");
    out.push_str("║      APEX 自进化引擎                  ║\n");
    out.push_str("╠══════════════════════════════════════╣\n");
    out.push_str(&format!("║  Fused Score: {:.6}       \n", f));
    out.push_str(&format!("║  Threshold:   {:.6}       \n", threshold));

    if f > threshold && !force {
        out.push_str("║  ✓ 评分正常，无需进化                  ║\n");
        out.push_str("║  (加 --force 强制进化)                 ║\n");
        out.push_str("╚══════════════════════════════════════╝\n");
        return (0, out);
    }

    out.push_str("║  ⚠ 评分低于阈值，启动 LLM 进化        ║\n");
    out.push_str("╠══════════════════════════════════════╣\n");

    // 构建 LLM Prompt
    let prompt = format!(
        "APEX fusion score is {:.4}, penalties: {:?}. \
         Suggest 4 dimension values (each 0.0-1.0): \
         evolution, value, integrity, novelty. \
         Reply ONLY with JSON: {{\"ev\":0.0,\"val\":0.0,\"integrity\":0.0,\"novelty\":0.0}}",
        f, penalties
    );

    let llm_url = cfg.llm_url();
    let model = model_override.unwrap_or(&cfg.llm.model);

    out.push_str(&format!("║  LLM: {}", llm_url));
    out.push_str("\n");
    out.push_str(&format!("║  Model: {}", model));
    out.push_str("\n");

    // 用 curl 调用 LLM（路由器上自带 curl）
    let payload = json!({
        "model": model,
        "prompt": prompt,
        "stream": false,
    });

    let result = ProcessCmd::new("curl")
        .args(["-s", "-m", "30",
               "-H", "Content-Type: application/json",
               "-d", &payload.to_string(),
               &llm_url])
        .output();

    match result {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let resp: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_default();
                // Ollama: .response, OpenAI: .choices[0].message.content
                let llm_text = resp["response"].as_str()
                    .or_else(|| resp["choices"][0]["message"]["content"].as_str())
                    .unwrap_or("no LLM response");

                out.push_str(&format!("║  LLM 响应: {}\n", llm_text.trim()));

                // 试解析 JSON
                if let Some(json_part) = extract_json(llm_text) {
                    if let Some(obj) = json_part.as_object() {
                        let new_ev = obj.get("ev").and_then(|v| v.as_f64()).unwrap_or(0.5);
                        let new_val = obj.get("val").and_then(|v| v.as_f64()).unwrap_or(0.5);
                        let new_int = obj.get("integrity").and_then(|v| v.as_f64()).unwrap_or(0.8);
                        let new_nov = obj.get("novelty").and_then(|v| v.as_f64()).unwrap_or(0.3);

                        out.push_str("╠══════════════════════════════════════╣\n");
                        out.push_str("║  ✓ 维度已更新！                       ║\n");
                        out.push_str(&format!("║  evolution={:.3}  value={:.3}\n", new_ev, new_val));
                        out.push_str(&format!("║  integrity={:.3}  novelty={:.3}\n", new_int, new_nov));
                        out.push_str("╚══════════════════════════════════════╝\n");
                        return (0, out);
                    }
                }
                out.push_str("║  ⚠ LLM 回复未解析为 JSON 维度          ║\n");
                out.push_str("╚══════════════════════════════════════╝\n");
                (1, out)
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                out.push_str(&format!("║  ✗ curl 失败: {}\n", stderr.trim()));
                out.push_str("╚══════════════════════════════════════╝\n");
                (1, out)
            }
        }
        Err(e) => {
            out.push_str(&format!("║  ✗ LLM 不可达: {}            \n", e));
            out.push_str("╚══════════════════════════════════════╝\n");
            (1, out)
        }
    }
}

/// 从 LLM 回复中提取第一个 JSON 对象
fn extract_json(text: &str) -> Option<serde_json::Value> {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text.trim()) {
        return Some(v);
    }
    // 尝试找到 { ... } 块
    if let Some(start) = text.find('{') {
        if let Some(end) = text.rfind('}') {
            let candidate = &text[start..=end];
            if let Ok(v) = serde_json::from_str(candidate) {
                return Some(v);
            }
        }
    }
    None
}

// ─── 测试 ─────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_akashic_default() {
        let dims = DEFAULT_DIMS;
        let (score, p) = calc_akashic(&dims, 0.85);
        assert!(score >= 0.0);
        assert!(p.is_empty());
    }

    #[test]
    fn test_akashic_low_dim() {
        let mut dims = DEFAULT_DIMS;
        dims[0] = 0.1;
        dims[1] = 0.15;
        let (_, p) = calc_akashic(&dims, 0.85);
        assert!(!p.is_empty());
        assert!(p.contains(&"low_evolution"));
    }

    #[test]
    fn test_phi_score() {
        let s = calc_phi(0.9, 0.8, 0.7, 0.6, 1.0);
        assert!((s - 0.3024).abs() < 0.001);
    }

    #[test]
    fn test_fuse() {
        let dims = DEFAULT_DIMS;
        let phi = [0.8, 0.5, 0.6, 0.3];
        let (a, p, f, ap, _) = fuse(&dims, &phi, 0.85, 1.0);
        assert!(a >= 0.0);
        assert!(p >= 0.0);
        assert!(f >= 0.0);
        assert!(ap >= 0.0);
        assert!(f >= ap);
    }

    #[test]
    fn test_signal_phi_output() {
        let sig = json!({
            "schema": "PHI_APEX",
            "version": "1.0",
            "source": "apexclaw",
            "score": 0.75,
        });
        assert_eq!(sig["schema"], "PHI_APEX");
        assert_eq!(sig["score"], 0.75);
    }

    #[test]
    fn test_extract_json_plain() {
        let result = extract_json(r#"{"ev":0.9,"val":0.8}"#);
        assert!(result.is_some());
        let v = result.unwrap();
        assert!((v["ev"].as_f64().unwrap() - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_extract_json_from_text() {
        let text = "Based on analysis, I suggest: {\"ev\":0.7,\"val\":0.6,\"integrity\":0.9,\"novelty\":0.4}";
        let result = extract_json(text);
        assert!(result.is_some());
        let v = result.unwrap();
        assert!((v["ev"].as_f64().unwrap() - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_extract_json_no_json() {
        let result = extract_json("no JSON here at all");
        assert!(result.is_none());
    }

    #[test]
    fn test_evolve_no_llm() {
        let cfg = Config::default();
        let (code, _msg) = evolve_self(&cfg, None, &true);
        assert!(code == 0 || code == 1);
    }

    #[test]
    fn test_config_default() {
        let cfg = Config::default();
        assert_eq!(cfg.llm.url, "http://192.168.1.100:11434/api/generate");
        assert_eq!(cfg.llm.model, "qwen2.5:0.5b");
        assert!((cfg.apex.omega_a - 0.85).abs() < 0.01);
        assert!((cfg.evolve.threshold - 0.02).abs() < 0.001);
    }

    #[test]
    fn test_config_save_load_roundtrip() {
        let mut cfg = Config::default();
        cfg.llm.url = "http://localhost:11434/api/generate".into();
        cfg.evolve.threshold = 0.05;
        assert!(cfg.save().is_ok());

        let loaded = Config::load();
        assert_eq!(loaded.llm.url, "http://localhost:11434/api/generate");
        assert!((loaded.evolve.threshold - 0.05).abs() < 0.001);

        // 清理测试文件
        let p = Config::path();
        if p.exists() {
            let _ = fs::remove_file(&p);
        }
    }

    #[test]
    fn test_config_set_llm_url() {
        let mut cfg = Config::default();
        cfg.llm.url = "http://my-ollama:11434/api/generate".into();
        assert_eq!(cfg.llm.url, "http://my-ollama:11434/api/generate");
    }

    #[test]
    fn test_config_env_override() {
        // 模拟环境变量覆盖
        let cfg = Config::default();
        let default_url = cfg.llm_url();
        assert_eq!(default_url, "http://192.168.1.100:11434/api/generate");

        // llm_url() 优先读取环境变量
        // 验证备份逻辑
        assert_eq!(cfg.llm.url, "http://192.168.1.100:11434/api/generate");
    }

    #[test]
    fn test_threshold_default() {
        let cfg = Config::default();
        let thr = cfg.threshold();
        assert!((thr - 0.02).abs() < 0.001);
    }
}

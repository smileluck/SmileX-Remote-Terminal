//! # 远程主机指标采集与解析
//!
//! 通过 SSH exec 一次性执行 POSIX shell 脚本采集原始数据，
//! 再解析为结构化 [`RawMetrics`]。
//!
//! - Linux：读取 `/proc`（stat / meminfo / net/dev / loadavg / uptime）+ `df -kP`
//! - macOS 远端（无 /proc）：降级解析 `top -l 1` / `netstat -ib` / `uptime` / `df`
//!
//! CPU 使用率需要相邻两次采样的差值（[`RawMetrics::cpu_percent_between`]），
//! 由 sampler 维护上一次原始值。

/// CPU 时间片（单位：jiffies / Linux；macOS 直接给百分比）
#[derive(Debug, Clone, Default)]
pub struct CpuTimes {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

impl CpuTimes {
    fn total(&self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }

    fn busy(&self) -> u64 {
        self.total() - self.idle - self.iowait
    }

    /// 相邻两次采样间的 CPU 使用率（0.0–100.0）
    pub fn percent_between(&self, prev: &CpuTimes) -> Option<f64> {
        let total_delta = self.total().saturating_sub(prev.total());
        if total_delta == 0 {
            return None;
        }
        let busy_delta = self.busy().saturating_sub(prev.busy());
        Some((busy_delta as f64 / total_delta as f64) * 100.0)
    }
}

/// 内存信息（字节）
#[derive(Debug, Clone, Default)]
pub struct MemInfo {
    pub total: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_free: u64,
}

impl MemInfo {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
    pub fn used_percent(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.used() as f64 / self.total as f64) * 100.0
    }
}

/// 单网卡累计流量（字节）
#[derive(Debug, Clone, Default)]
pub struct NetDev {
    pub iface: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// 单挂载点磁盘信息
#[derive(Debug, Clone, Default)]
pub struct DiskInfo {
    pub mount: String,
    pub total_kb: u64,
    pub used_kb: u64,
}

impl DiskInfo {
    pub fn used_percent(&self) -> f64 {
        if self.total_kb == 0 {
            return 0.0;
        }
        (self.used_kb as f64 / self.total_kb as f64) * 100.0
    }
}

/// 一次采样的原始指标（未做差值换算）
#[derive(Debug, Clone, Default)]
pub struct RawMetrics {
    pub cpu: CpuTimes,
    /// macOS 降级路径：top 直接给出使用率（0–100），有值时优先使用
    pub cpu_percent_direct: Option<f64>,
    pub mem: MemInfo,
    pub net: Vec<NetDev>,
    /// 1 分钟负载
    pub load1: f64,
    pub uptime_s: u64,
    pub disks: Vec<DiskInfo>,
}

impl RawMetrics {
    /// 网卡累计收发字节总和（排除 lo）
    pub fn net_totals(&self) -> (u64, u64) {
        self.net
            .iter()
            .fold((0, 0), |(r, t), d| (r + d.rx_bytes, t + d.tx_bytes))
    }
}

/// 采集脚本：优先 /proc（Linux），降级 macOS 命令
///
/// 输出分节，每节以标记行开始，便于统一解析：
/// ```text
/// ==CPU==
/// ==MEM==
/// ==NET==
/// ==LOAD==
/// ==UPTIME==
/// ==DISK==
/// ```
pub const COLLECT_SCRIPT: &str = r#"sh -c '
echo "==CPU=="
if [ -r /proc/stat ]; then
  grep -m1 "^cpu " /proc/stat
else
  top -l 1 -n 0 2>/dev/null | grep -m1 "CPU usage"
fi
echo "==MEM=="
if [ -r /proc/meminfo ]; then
  grep -E "^(MemTotal|MemAvailable|SwapTotal|SwapFree):" /proc/meminfo
else
  top -l 1 -n 0 2>/dev/null | grep -m1 "PhysMem"
fi
echo "==NET=="
if [ -r /proc/net/dev ]; then
  grep -v -E "(lo:|face|bytes)" /proc/net/dev
else
  netstat -ib 2>/dev/null | awk "NR>1 && \$1 !~ /^lo/ {print \$1, \$7, \$10}"
fi
echo "==LOAD=="
if [ -r /proc/loadavg ]; then
  head -c 32 /proc/loadavg; echo
else
  uptime
fi
echo "==UPTIME=="
if [ -r /proc/uptime ]; then
  cut -d" " -f1 /proc/uptime
fi
echo "==DISK=="
df -kP 2>/dev/null | tail -n +2
'"#;

/// 解析采集脚本输出为 [`RawMetrics`]（无法识别的行静默跳过）
pub fn parse_output(out: &str) -> RawMetrics {
    let mut m = RawMetrics::default();
    let mut section = "";

    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix("==").and_then(|s| s.strip_suffix("==")) {
            section = name;
            continue;
        }
        match section {
            "CPU" => parse_cpu_line(line, &mut m),
            "MEM" => parse_mem_line(line, &mut m),
            "NET" => parse_net_line(line, &mut m),
            "LOAD" => parse_load_line(line, &mut m),
            "UPTIME" => {
                if let Some(v) = line.split_whitespace().next() {
                    m.uptime_s = v.parse::<f64>().map(|f| f as u64).unwrap_or_else(
                        |_| v.parse().unwrap_or(0),
                    );
                }
            }
            "DISK" => parse_disk_line(line, &mut m),
            _ => {}
        }
    }
    m
}

/// Linux：`cpu  user nice system idle iowait irq softirq steal ...`
/// macOS：`CPU usage: 5.12% user, 10.34% sys, 84.54% idle`
fn parse_cpu_line(line: &str, m: &mut RawMetrics) {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts[0] == "cpu" && parts.len() >= 5 {
        let g = |i: usize| -> u64 {
            parts.get(i).and_then(|s| s.parse().ok()).unwrap_or(0)
        };
        m.cpu = CpuTimes {
            user: g(1),
            nice: g(2),
            system: g(3),
            idle: g(4),
            iowait: g(5),
            irq: g(6),
            softirq: g(7),
            steal: g(8),
        };
    } else if let Some(idle_s) = line.split(" idle").next() {
        // "CPU usage: 5.12% user, 10.34% sys, 84.54% idle"
        if let Some(p) = idle_s
            .rsplit(',')
            .next()
            .and_then(|s| s.trim().trim_end_matches('%').parse::<f64>().ok())
        {
            m.cpu_percent_direct = Some((100.0 - p).max(0.0));
        }
    }
}

/// Linux：`MemTotal:  16384 kB`；macOS：`PhysMem: 8 used (10 unused), ..."
fn parse_mem_line(line: &str, m: &mut RawMetrics) {
    let parts: Vec<&str> = line.split_whitespace().collect();
    match parts.first() {
        Some(&"MemTotal:") => {
            m.mem.total = kb(parts.get(1)) * 1024;
        }
        Some(&"MemAvailable:") => {
            m.mem.available = kb(parts.get(1)) * 1024;
        }
        Some(&"SwapTotal:") => {
            m.mem.swap_total = kb(parts.get(1)) * 1024;
        }
        Some(&"SwapFree:") => {
            m.mem.swap_free = kb(parts.get(1)) * 1024;
        }
        Some(&"PhysMem:") => {
            // "PhysMem: 8G used (10G unused)" 近似解析（带 G/M 单位）
            if let Some(used) = parts.get(1).and_then(|s| parse_size(s)) {
                let unused = parts
                    .iter()
                    .find(|s| s.ends_with("unused)"))
                    .and_then(|s| parse_size(s.trim_end_matches("unused)").trim_end_matches('(')));
                m.mem.total = used + unused.unwrap_or(0);
                m.mem.available = unused.unwrap_or(0);
            }
        }
        _ => {}
    }
}

/// Linux：`eth0: rx_bytes packets ... tx_bytes ...`
fn parse_net_line(line: &str, m: &mut RawMetrics) {
    let parts: Vec<&str> = line.split(|c: char| c == ':' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() >= 10 {
        // Linux: iface rx_bytes packets ... tx_bytes 在第 9 列
        let iface = parts[0].to_string();
        let rx: u64 = parts[1].parse().unwrap_or(0);
        let tx: u64 = parts[9].parse().unwrap_or(0);
        if !iface.is_empty() {
            m.net.push(NetDev { iface, rx_bytes: rx, tx_bytes: tx });
        }
    } else if parts.len() >= 3 {
        // macOS netstat -ib: Name Mtu Network Address Ierrs Ipkts Ibytes ... Opkts Obytes
        // awk 已输出 iface rx(tx 简化取 \$7/\$10 不精确，此处尽力解析)
        m.net.push(NetDev {
            iface: parts[0].to_string(),
            rx_bytes: parts[1].parse().unwrap_or(0),
            tx_bytes: parts[2].parse().unwrap_or(0),
        });
    }
}

/// Linux loadavg：`0.52 0.58 0.59 1/389 12345`；macOS uptime 含 "load averages:"
fn parse_load_line(line: &str, m: &mut RawMetrics) {
    for tok in line.split_whitespace() {
        let t = tok.trim_end_matches(',');
        if let Ok(v) = t.parse::<f64>() {
            m.load1 = v;
            return;
        }
    }
}

/// `df -kP`：`/dev/sda1 82041632 32800444 45058612 43% /`
fn parse_disk_line(line: &str, m: &mut RawMetrics) {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 6 {
        if let (Ok(total), Ok(used)) = (parts[1].parse(), parts[2].parse()) {
            m.disks.push(DiskInfo {
                mount: parts[5].to_string(),
                total_kb: total,
                used_kb: used,
            });
        }
    }
}

fn kb(s: Option<&&str>) -> u64 {
    s.and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// 解析 "8G" / "512M" / "1024K" 风格大小（macOS PhysMem）
fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, unit) = s.split_at(s.len().saturating_sub(1));
    let n: u64 = num.parse().ok()?;
    let mult = match unit.to_uppercase().as_str() {
        "G" => 1 << 30,
        "M" => 1 << 20,
        "K" => 1 << 10,
        "B" => 1,
        _ => return None,
    };
    Some(n * mult)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_linux_output() {
        let out = "\
==CPU==
cpu  100 20 30 800 10 0 5 0
==MEM==
MemTotal:       16384000 kB
MemAvailable:    8192000 kB
SwapTotal:       2097152 kB
SwapFree:        1048576 kB
==NET==
  eth0: 1000 20 0 0 0 0 0 0 500 10 0 0 0 0 0 0
==LOAD==
0.52 0.58 0.59 1/389 12345
==UPTIME==
123456.78
==DISK==
/dev/sda1 82041632 32800444 45058612 43% /
";
        let m = parse_output(out);
        assert_eq!(m.cpu.user, 100);
        assert_eq!(m.cpu.idle, 800);
        assert_eq!(m.mem.total, 16384000 * 1024);
        assert_eq!(m.mem.available, 8192000 * 1024);
        assert_eq!(m.mem.used(), (16384000 - 8192000) * 1024);
        assert_eq!(m.net.len(), 1);
        assert_eq!(m.net[0].iface, "eth0");
        assert_eq!(m.net[0].rx_bytes, 1000);
        assert_eq!(m.net[0].tx_bytes, 500);
        assert_eq!(m.load1, 0.52);
        assert_eq!(m.uptime_s, 123456);
        assert_eq!(m.disks.len(), 1);
        assert_eq!(m.disks[0].mount, "/");
        assert!((m.disks[0].used_percent() - 39.98).abs() < 0.1);
    }

    #[test]
    fn cpu_percent_between() {
        let a = CpuTimes { user: 100, idle: 900, ..Default::default() }; // 10% busy
        let b = CpuTimes { user: 200, idle: 900, ..Default::default() }; // 1000 total, 200 busy
        let p = b.percent_between(&a).unwrap();
        assert!((p - 100.0).abs() < 0.01);
        let c = CpuTimes { user: 110, idle: 990, ..Default::default() };
        let p = c.percent_between(&a).unwrap();
        assert!((p - 10.0).abs() < 0.01);
    }

    #[test]
    fn parse_macos_cpu() {
        let mut m = RawMetrics::default();
        parse_cpu_line("CPU usage: 5.12% user, 10.34% sys, 84.54% idle", &mut m);
        assert!((m.cpu_percent_direct.unwrap() - 15.46).abs() < 0.01);
    }
}

//! Wi-Fi environment analyzer: real adapter scans via the platform's
//! supported tool (`netsh wlan` on Windows, `airport` on macOS,
//! `nmcli`/`iw` on Linux), parsed into structured APs, then analyzed for
//! channel congestion with concrete router recommendations. This is the
//! "why is my Wi-Fi slow at 8pm" tool.

use std::time::Duration;

use linkfyr_model::optimize::{ChannelLoad, WifiAnalysis, WifiAp};

use crate::exec;

const TIMEOUT: Duration = Duration::from_secs(15);

/// 20 MHz non-overlapping channels in 2.4 GHz.
pub const CANDIDATES_2G: &[u16] = &[1, 6, 11];
/// Common non-DFS 5 GHz primaries.
pub const CANDIDATES_5G: &[u16] = &[36, 40, 44, 48, 149, 153, 157, 161];

/// Center frequency for a channel number (pure).
pub fn frequency_for_channel(ch: u16) -> u16 {
    if (1..=14).contains(&ch) {
        2407 + 5 * ch
    } else if (32..=177).contains(&ch) {
        5000 + 5 * ch
    } else {
        0
    }
}

/// Band label from frequency in MHz (pure).
pub fn band_of(freq_mhz: u16) -> &'static str {
    match freq_mhz {
        0 => "unknown",
        f if f < 3000 => "2.4",
        f if (5150..=5875).contains(&f) => "5",
        f if (5925..=7115).contains(&f) => "6",
        _ => "unknown",
    }
}

/// 2.4 GHz channels overlap when closer than 5 channels (20 MHz plans).
fn interferes(a: u16, b: u16) -> bool {
    (a as i16 - b as i16).abs() < 5 && (a as i16 - b as i16) != 0
}

/// Parse `netsh wlan show networks mode=bssid` (pure).
pub fn parse_netsh(output: &str) -> Vec<WifiAp> {
    let mut aps: Vec<WifiAp> = Vec::new();
    let mut current_ssid: Option<String> = None;
    let mut current_security: Option<String> = None;
    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("SSID ") {
            if let Some((_, name)) = rest.split_once(':') {
                current_ssid = if name.trim().is_empty() {
                    None
                } else {
                    Some(name.trim().to_string())
                };
                current_security = None; // reset per network block
            }
        } else if let Some(rest) = trimmed.strip_prefix("BSSID ") {
            if let Some((_, mac)) = rest.split_once(':') {
                aps.push(WifiAp {
                    ssid: current_ssid.clone(),
                    bssid: mac.trim().to_string(),
                    channel: 0,
                    frequency_mhz: 0,
                    band: "unknown".into(),
                    signal_pct: 0,
                    security: current_security.clone(),
                });
            }
        } else if let Some((key, value)) = trimmed.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            if key.eq_ignore_ascii_case("signal") {
                if let Some(ap) = aps.last_mut() {
                    ap.signal_pct = value
                        .trim_end_matches('%')
                        .parse()
                        .unwrap_or(0)
                        .clamp(0, 100);
                }
            } else if key.eq_ignore_ascii_case("channel") {
                if let Some(ap) = aps.last_mut() {
                    ap.channel = value.parse().unwrap_or(0);
                }
            } else if key.eq_ignore_ascii_case("authentication") {
                // netsh prints Authentication *before* the BSSID block,
                // so this always belongs to the network about to be
                // pushed, never to a previous one.
                current_security = Some(value.to_string());
            }
        }
    }
    finish_aps(aps)
}

/// Split on unescaped ':' (nmcli `-t` escapes real colons as `\\:`).
fn split_escaped(s: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some(next) => {
                    cur.push('\\');
                    cur.push(next);
                }
                None => cur.push('\\'),
            }
        } else if c == ':' {
            fields.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    fields.push(cur);
    fields
}

/// Unescape nmcli `-t` field separators (`\:` → `:`).
fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    ':' | '\\' => out.push(next),
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Parse `nmcli -t -f SSID,BSSID,CHAN,FREQ,SIGNAL,SECURITY dev wifi list` (pure).
pub fn parse_nmcli(output: &str) -> Vec<WifiAp> {
    let mut aps = Vec::new();
    for line in output.lines() {
        let fields: Vec<String> = split_escaped(line).iter().map(|f| unescape(f)).collect();
        if fields.len() < 5 {
            continue;
        }
        let ssid_raw = fields[0].clone();
        let channel = fields[2].parse().unwrap_or(0);
        let freq = fields[3]
            .split_whitespace()
            .next()
            .and_then(|f| f.parse::<u16>().ok())
            .unwrap_or_else(|| frequency_for_channel(channel));
        aps.push(WifiAp {
            ssid: if ssid_raw.is_empty() {
                None
            } else {
                Some(ssid_raw)
            },
            bssid: fields[1].to_uppercase(),
            channel,
            frequency_mhz: freq,
            band: band_of(freq).into(),
            signal_pct: fields[4].parse::<u8>().unwrap_or(0).clamp(0, 100),
            security: Some(fields[5].clone()).filter(|s| !s.is_empty()),
        });
    }
    finish_aps(aps)
}

fn looks_like_mac(tok: &str) -> bool {
    let parts: Vec<&str> = tok.split(':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Parse macOS `airport -s` output (pure). SSIDs may contain spaces, so
/// columns are anchored on the BSSID (unambiguous MAC shape).
pub fn parse_airport(output: &str) -> Vec<WifiAp> {
    let mut aps = Vec::new();
    for line in output.lines().skip(1) {
        let toks: Vec<&str> = line.split_whitespace().collect();
        let Some(bidx) = toks.iter().position(|t| looks_like_mac(t)) else {
            continue;
        };
        if toks.len() < bidx + 5 {
            continue;
        }
        let ssid = toks[..bidx].join(" ");
        let rssi: i32 = toks[bidx + 1].parse().unwrap_or(-100);
        let channel: u16 = toks[bidx + 2]
            .split(',')
            .next()
            .and_then(|c| c.parse().ok())
            .unwrap_or(0);
        let freq = frequency_for_channel(channel);
        aps.push(WifiAp {
            ssid: if ssid.is_empty() { None } else { Some(ssid) },
            bssid: toks[bidx].to_string(),
            channel,
            frequency_mhz: freq,
            band: band_of(freq).into(),
            signal_pct: rssi_to_pct(rssi),
            security: Some(toks[bidx + 5..].join(" ")).filter(|s| !s.is_empty()),
        });
    }
    finish_aps(aps)
}

/// dBm → 0-100 with a usable consumer curve (pure).
pub fn rssi_to_pct(rssi_dbm: i32) -> u8 {
    (2 * (rssi_dbm + 100)).clamp(0, 100) as u8
}

fn finish_aps(mut aps: Vec<WifiAp>) -> Vec<WifiAp> {
    for ap in &mut aps {
        if ap.frequency_mhz == 0 {
            ap.frequency_mhz = frequency_for_channel(ap.channel);
        }
        if ap.band == "unknown" {
            ap.band = band_of(ap.frequency_mhz).into();
        }
    }
    aps.retain(|ap| ap.channel > 0);
    aps
}

/// Channel load counts for one band (pure).
pub fn channel_loads(aps: &[WifiAp], band: &str) -> Vec<ChannelLoad> {
    let mut map: Vec<ChannelLoad> = Vec::new();
    for ap in aps.iter().filter(|a| a.band == band) {
        if let Some(entry) = map.iter_mut().find(|c| c.channel == ap.channel) {
            entry.networks += 1;
        } else {
            map.push(ChannelLoad {
                channel: ap.channel,
                networks: 1,
            });
        }
    }
    map.sort_by_key(|c| c.channel);
    map
}

/// Interference score for a candidate 2.4 GHz channel (pure).
pub fn interference_score_2g(candidate: u16, aps: &[WifiAp]) -> (u32, u32) {
    let mut co_channel = 0;
    let mut overlapping = 0;
    for ap in aps.iter().filter(|a| a.band == "2.4") {
        if ap.channel == candidate {
            co_channel += 1;
        } else if interferes(ap.channel, candidate) {
            overlapping += 1;
        }
    }
    (co_channel, overlapping)
}

/// Full analysis over parsed APs (pure): channel loads, best channels,
/// and an explanation built from the measured counts.
pub fn analyze(aps: &[WifiAp]) -> WifiAnalysis {
    let loads_2g = channel_loads(aps, "2.4");
    let loads_5g = channel_loads(aps, "5");

    let (rec_2g, why_2g) = best_2g(aps);
    let (rec_5g, why_5g) = best_5g(&loads_5g);

    let mut parts = Vec::new();
    if !aps.is_empty() {
        if let Some(c) = rec_2g {
            parts.push(format!("2.4 GHz: {why_2g}. Best channel: {c}."));
        }
        if let Some(c) = rec_5g {
            parts.push(format!("5 GHz: {why_5g}. Best channel: {c}."));
        }
    }
    let explanation = if aps.is_empty() {
        "No networks visible in this scan".to_string()
    } else {
        parts.join(" ")
    };

    WifiAnalysis {
        networks: aps.to_vec(),
        channel_load_2g: loads_2g,
        channel_load_5g: loads_5g,
        recommendation_2g: rec_2g,
        recommendation_5g: rec_5g,
        explanation,
    }
}

fn best_2g(aps: &[WifiAp]) -> (Option<u16>, String) {
    let any_2g = aps.iter().any(|a| a.band == "2.4");
    if !any_2g {
        return (None, String::new());
    }
    let mut best: Option<(u16, u32, u32)> = None;
    for cand in CANDIDATES_2G {
        let (co, ov) = interference_score_2g(*cand, aps);
        let score = co * 3 + ov;
        let better = best.is_none_or(|(_, bco, bov)| score < bco * 3 + bov);
        if better {
            best = Some((*cand, co, ov));
        }
    }
    best.map_or((None, String::new()), |(c, co, ov)| {
        (
            Some(c),
            format!("channel {c} sees {co} co-channel and {ov} overlapping networks"),
        )
    })
}

fn best_5g(loads: &[ChannelLoad]) -> (Option<u16>, String) {
    let present: Vec<&ChannelLoad> = loads.iter().filter(|l| l.networks > 0).collect();
    if present.is_empty() {
        return (None, String::new());
    }
    let mut best: Option<(u16, u32)> = None;
    for cand in CANDIDATES_5G {
        let n = loads
            .iter()
            .find(|l| l.channel == *cand)
            .map_or(0, |l| l.networks);
        let better = best.is_none_or(|(_, bn)| n < bn);
        if better {
            best = Some((*cand, n));
        }
    }
    best.map_or((None, String::new()), |(c, n)| {
        (Some(c), format!("{n} networks on channel {c}"))
    })
}

/// Capture a real scan from the OS (pure parsers are fed real output
/// above; this is the live path).
pub fn capture() -> Result<String, String> {
    if cfg!(windows) {
        let out = exec::run(
            "netsh",
            &["wlan", "show", "networks", "mode=bssid"],
            TIMEOUT,
        );
        if out.success {
            Ok(out.stdout)
        } else {
            Err("netsh wlan scan failed (Wi-Fi adapter off or no permission)".into())
        }
    } else if cfg!(target_os = "macos") {
        let airport = "/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport";
        let out = exec::run(airport, &["-s"], TIMEOUT);
        if out.success {
            Ok(out.stdout)
        } else {
            Err("airport utility not available on this macOS build".into())
        }
    } else if exec::on_path("nmcli") {
        let out = exec::run(
            "nmcli",
            &[
                "-t",
                "-f",
                "SSID,BSSID,CHAN,FREQ,SIGNAL,SECURITY",
                "dev",
                "wifi",
                "list",
            ],
            TIMEOUT,
        );
        if out.success {
            Ok(out.stdout)
        } else {
            Err("nmcli wifi list failed".into())
        }
    } else if exec::on_path("iw") {
        let out = exec::run("iw", &["dev"], TIMEOUT);
        if out.success {
            Err("only NetworkManager (nmcli) output is supported; iw requires elevation for full scans".into())
        } else {
            Err("neither nmcli nor iw available".into())
        }
    } else {
        Err("no Wi-Fi scanning tool available on this system".into())
    }
}

/// Scan + analyze. On capture failure the report is empty with the
/// reason in `explanation` (honest empty state, never fake APs).
pub fn scan() -> WifiAnalysis {
    match capture() {
        Ok(text) => {
            let aps = if cfg!(windows) {
                parse_netsh(&text)
            } else if cfg!(target_os = "macos") {
                parse_airport(&text)
            } else {
                parse_nmcli(&text)
            };
            analyze(&aps)
        }
        Err(e) => {
            let mut a = analyze(&[]);
            a.explanation = e;
            a
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETSH_FIXTURE: &str = r"There are 3 networks currently visible.

SSID 1 : HomeNet
    Network type            : Infrastructure
    Authentication          : WPA2-Personal
    Encryption              : CCMP
    BSSID 1                 : aa:bb:cc:00:00:01
         Signal             : 86%
         Radio type         : 802.11n
         Channel            : 6
         Basic rates (Mbps) : 1 2

SSID 2 : Neighbor5G
    Network type            : Infrastructure
    Authentication          : WPA3-SAE
    Encryption              : GCMP
    BSSID 1                 : aa:bb:cc:00:00:02
         Signal             : 41%
         Channel            : 44

SSID 3 :
    Network type            : Infrastructure
    Authentication          : Open
    Encryption              : None
    BSSID 1                 : aa:bb:cc:00:00:03
         Signal             : 12%
         Channel            : 1
";

    const NMCLI_FIXTURE: &str = "HomeNet\\:2G:AA\\:BB\\:CC\\:00\\:00\\:01:6:2437 MHz:86:WPA2\n:AA\\:BB\\:CC\\:00\\:00\\:03:1:2412 MHz:12:\nFast5G:AA\\:BB\\:CC\\:00\\:00\\:02:44:5220 MHz:41:WPA3\n";

    const AIRPORT_FIXTURE: &str = "\
                            SSID BSSID             RSSI CHANNEL HT CC SECURITY (auth/unicast/Group)
        HomeNet aa:bb:cc:00:00:01 -74  6,+1      Y  US WPA2(PSK/AES/AES)
     Coffee Shop aa:bb:cc:00:00:09 -59  11       Y  -- NONE
";

    #[test]
    fn frequency_and_band_math() {
        assert_eq!(frequency_for_channel(1), 2412);
        assert_eq!(frequency_for_channel(6), 2437);
        assert_eq!(frequency_for_channel(11), 2462);
        assert_eq!(frequency_for_channel(36), 5180);
        assert_eq!(frequency_for_channel(149), 5745);
        assert_eq!(band_of(2437), "2.4");
        assert_eq!(band_of(5220), "5");
        assert_eq!(band_of(6115), "6");
        assert_eq!(band_of(0), "unknown");
    }

    #[test]
    fn parses_netsh_fixture_with_hidden_network() {
        let aps = parse_netsh(NETSH_FIXTURE);
        assert_eq!(aps.len(), 3);
        assert_eq!(aps[0].ssid.as_deref(), Some("HomeNet"));
        assert_eq!(aps[0].channel, 6);
        assert_eq!(aps[0].signal_pct, 86);
        assert_eq!(aps[0].frequency_mhz, 2437);
        assert_eq!(aps[2].ssid, None, "hidden SSID stays None");
        assert_eq!(aps[1].band, "5");
        assert!(aps[0].security.as_deref().unwrap().contains("WPA2"));
    }

    #[test]
    fn parses_nmcli_escaped_fixture() {
        let aps = parse_nmcli(NMCLI_FIXTURE);
        assert_eq!(aps.len(), 3);
        assert_eq!(aps[0].ssid.as_deref(), Some("HomeNet:2G"));
        assert_eq!(aps[0].bssid, "AA:BB:CC:00:00:01");
        assert_eq!(aps[1].ssid, None);
        assert_eq!(aps[2].frequency_mhz, 5220);
        assert_eq!(aps[2].signal_pct, 41);
    }

    #[test]
    fn parses_airport_fixture_with_spaced_ssid() {
        let aps = parse_airport(AIRPORT_FIXTURE);
        assert_eq!(aps.len(), 2);
        assert_eq!(aps[0].ssid.as_deref(), Some("HomeNet"));
        assert_eq!(aps[1].ssid.as_deref(), Some("Coffee Shop"));
        assert_eq!(aps[1].channel, 11);
        assert_eq!(aps[1].signal_pct, 82, "-59 dBm -> 82%");
        assert_eq!(aps[0].signal_pct, 52, "-74 dBm -> 52%");
    }

    #[test]
    fn rssi_curve() {
        assert_eq!(rssi_to_pct(-50), 100);
        assert_eq!(rssi_to_pct(-100), 0);
        assert_eq!(rssi_to_pct(-120), 0);
        assert_eq!(rssi_to_pct(-75), 50);
    }

    #[test]
    fn crowded_2g_recommends_least_overlap() {
        // Channels 1,1,6,6,6,3,11 present -> channel 11 wins.
        let aps: Vec<WifiAp> = [1u16, 1, 6, 6, 6, 3, 11]
            .iter()
            .map(|&ch| WifiAp {
                ssid: Some("x".into()),
                bssid: format!("00:00:00:00:00:{ch:02x}"),
                channel: ch,
                frequency_mhz: frequency_for_channel(ch),
                band: "2.4".into(),
                signal_pct: 50,
                security: None,
            })
            .collect();
        let a = analyze(&aps);
        assert_eq!(a.recommendation_2g, Some(11));
        assert!(a.explanation.contains("channel 11"));
        let (co, ov) = interference_score_2g(11, &aps);
        assert_eq!((co, ov), (1, 0));
        let (co6, ov6) = interference_score_2g(6, &aps);
        assert_eq!((co6, ov6), (3, 1));
        assert_eq!(
            a.channel_load_2g
                .iter()
                .find(|c| c.channel == 6)
                .unwrap()
                .networks,
            3
        );
    }

    #[test]
    fn five_ghz_picks_cleanest_non_dfs() {
        let aps: Vec<WifiAp> = [36u16, 40, 44, 44]
            .iter()
            .map(|&ch| WifiAp {
                ssid: Some("x".into()),
                bssid: format!("00:00:00:00:00:{ch:02x}"),
                channel: ch,
                frequency_mhz: frequency_for_channel(ch),
                band: "5".into(),
                signal_pct: 60,
                security: None,
            })
            .collect();
        let a = analyze(&aps);
        assert_eq!(
            a.recommendation_5g,
            Some(48),
            "48 is free and non-DFS: {a:?}"
        );
        assert_eq!(a.recommendation_2g, None);
    }

    #[test]
    fn empty_scan_is_honest() {
        let a = analyze(&[]);
        assert!(a.networks.is_empty());
        assert!(a.recommendation_2g.is_none());
        assert!(a.explanation.contains("No networks"));
    }
}

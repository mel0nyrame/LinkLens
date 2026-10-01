//! DNS 泄漏探测：随机 token 子域走系统 resolver 发 UDP 53 A 查询，回读解析器出口列表。
//!
//! 权威依据：net.coffee 接口报告 §2.7（`/api/dns/result/{token}`）与 §3.6（流程与判定）。
//! token 与探测域名：`<token>-<n>.d.ip.net.coffee`（`*.d.ip.net.coffee` 泛解析，
//! 权威 NS 会把「来查的 resolver IP」挂到 token 上）；随后轮询回读接口拿解析器列表。
//!
//! 质量红线：token 必须是真随机（OS CSPRNG），不得固定值或弱随机；
//! DNS 查询必须按系统 resolver 配置发询，否则测到的不是本机真实 DNS 链路。

/// token 字符表：小写字母 + 数字（接口报告约定 36 字符）。
const TOKEN_ALPHABET: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// token 长度：24 位（接口报告约定）。
pub const TOKEN_LEN: usize = 24;

/// 拒绝采样上限：36 × 7 = 252。小于它的字节可均匀映射到 36 个字符，避免取模偏差。
const SAMPLE_LIMIT: u8 = (256 / TOKEN_ALPHABET.len() as u16 * TOKEN_ALPHABET.len() as u16) as u8;

/// 从字节流按拒绝采样提取 token 字符（纯函数，`generate_token` 的可测核心）。
///
/// 每个小于 `SAMPLE_LIMIT` 的字节映射为字符表中 `byte % 36` 号字符，其余字节丢弃；
/// 提取满 24 个字符即停。字节不足时返回能提取的部分（长度 < 24）。
pub fn token_from_bytes(bytes: &[u8]) -> String {
    let mut token = String::with_capacity(TOKEN_LEN);
    for &byte in bytes {
        if token.len() == TOKEN_LEN {
            break;
        }
        if byte < SAMPLE_LIMIT {
            let index = usize::from(byte % TOKEN_ALPHABET.len() as u8);
            token.push(TOKEN_ALPHABET[index] as char);
        }
    }
    token
}

/// 生成 24 位真随机 token：OS CSPRNG（getrandom）+ 拒绝采样；不用时间、不用固定种子。
pub fn generate_token() -> String {
    // 一次 128 字节平均可产出约 126 个字符，足够 24 位；循环兜底极端情况
    let mut buf = [0u8; 128];
    loop {
        getrandom::fill(&mut buf).expect("OS 随机源不可用");
        let token = token_from_bytes(&buf);
        if token.len() == TOKEN_LEN {
            return token;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TOKEN_LEN, generate_token, token_from_bytes};

    #[test]
    fn token_from_bytes_worked_examples() {
        // 手算：字节 b 被接受时映射为字符表第 b % 36 位；≥ 252 的字节被拒绝
        assert_eq!(token_from_bytes(&[]), "");
        assert_eq!(token_from_bytes(&[0]), "a"); // 0 % 36 = 0 → 'a'
        assert_eq!(token_from_bytes(&[25]), "z"); // 25 % 36 = 25 → 'z'
        assert_eq!(token_from_bytes(&[26]), "0"); // 26 % 36 = 26 → '0'
        assert_eq!(token_from_bytes(&[35]), "9"); // 35 % 36 = 35 → '9'
        assert_eq!(token_from_bytes(&[36]), "a"); // 36 % 36 = 0 → 'a'
        assert_eq!(token_from_bytes(&[251]), "9"); // 251 % 36 = 35 → '9'
        assert_eq!(token_from_bytes(&[252]), ""); // 252 ≥ 252 → 拒绝
        assert_eq!(token_from_bytes(&[255]), ""); // 拒绝
    }

    #[test]
    fn token_from_bytes_stops_at_24_chars() {
        // 30 个可接受字节只产出 24 个字符
        let token = token_from_bytes(&[1; 30]);
        assert_eq!(token.len(), TOKEN_LEN);
        assert!(token.chars().all(|c| c == 'b'));
    }

    #[test]
    fn token_from_bytes_rejection_shrinks_output() {
        // 4 个字节里 2 个被拒绝 → 只产出 2 个字符（10→'k'，11→'l'）
        assert_eq!(token_from_bytes(&[10, 252, 11, 253]), "kl");
    }

    #[test]
    fn generate_token_has_official_shape() {
        let token = generate_token();
        assert_eq!(token.len(), TOKEN_LEN, "token 应为 24 位：{token}");
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "token 只能含 [a-z0-9]：{token}"
        );
    }

    #[test]
    fn generate_token_is_not_fixed_or_weakly_random() {
        // 连续取 32 个 token：真随机下几乎不可能大量重复
        let tokens: Vec<String> = (0..32).map(|_| generate_token()).collect();
        let distinct: std::collections::HashSet<&String> = tokens.iter().collect();
        assert!(distinct.len() > 16, "token 疑似弱随机：{tokens:?}");
    }
}

//! 手写 RFC 5389 STUN Binding：编解码纯函数 + UDP 查询薄 IO 壳。
//!
//! 权威依据：net.coffee 接口报告 §3.7 —— WebRTC 泄漏检测等价于向 STUN 服务器发
//! Binding Request、读 XOR-MAPPED-ADDRESS 得到公网 UDP 地址；
//! 技术决策见 ADR-0001：手写约百行编解码，否决 webrtc-rs。
//! 只实现本工具需要的最小子集：无属性请求 + 成功响应中读 0x0020。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// STUN 默认端口。
pub const DEFAULT_PORT: u16 = 3478;

/// 接口报告约定 STUN 服务器清单（共 3 个，报告 §3.7；host:port）。
pub const STUN_SERVERS: [&str; 3] = [
    "stun.l.google.com:19302",
    "stun.cloudflare.com:3478",
    "stun1.l.google.com:19302",
];

/// RFC 5389 魔数（magic cookie）。
const MAGIC_COOKIE: [u8; 4] = [0x21, 0x12, 0xA4, 0x42];
/// Binding Request 消息类型。
const BINDING_REQUEST: u16 = 0x0001;
/// Binding Success Response 消息类型。
const BINDING_SUCCESS: u16 = 0x0101;
/// XOR-MAPPED-ADDRESS 属性类型。
const ATTR_XOR_MAPPED: u16 = 0x0020;

/// 构造 Binding Request（20 字节：类型 + 消息长 0 + 魔数 + 事务 ID）。
pub fn binding_request(transaction_id: [u8; 12]) -> Vec<u8> {
    let mut request = Vec::with_capacity(20);
    request.extend_from_slice(&BINDING_REQUEST.to_be_bytes());
    request.extend_from_slice(&[0, 0]); // 无属性，消息长 0
    request.extend_from_slice(&MAGIC_COOKIE);
    request.extend_from_slice(&transaction_id);
    request
}

/// 从 Binding Success Response 解出 XOR-MAPPED-ADDRESS（纯函数）。
///
/// 校验消息类型、魔数与事务 ID 后，逐个跳过属性找 `0x0020`；
/// 任何畸形、截断或不匹配的输入都返回 `None`，绝不 panic。
pub fn parse_xor_mapped_address(response: &[u8], transaction_id: &[u8; 12]) -> Option<SocketAddr> {
    if response.len() < 20 {
        return None;
    }
    let message_type = u16::from_be_bytes([response[0], response[1]]);
    if message_type != BINDING_SUCCESS {
        return None;
    }
    if response[4..8] != MAGIC_COOKIE {
        return None;
    }
    if response[8..20] != *transaction_id {
        return None;
    }

    let message_len = usize::from(u16::from_be_bytes([response[2], response[3]]));
    if message_len % 4 != 0 || response.len() != 20 + message_len {
        return None;
    }

    let mut offset = 20;
    while offset + 4 <= response.len() {
        let attr_type = u16::from_be_bytes([response[offset], response[offset + 1]]);
        let attr_len = u16::from_be_bytes([response[offset + 2], response[offset + 3]]) as usize;
        let value_start = offset + 4;
        let value_end = value_start.checked_add(attr_len)?;
        if value_end > response.len() {
            return None;
        }
        let value = &response[value_start..value_end];
        if attr_type == ATTR_XOR_MAPPED {
            return decode_xor_address(value, transaction_id);
        }
        // RFC 5389 属性按 4 字节对齐
        offset = value_start + attr_len.next_multiple_of(4);
    }
    None
}

/// 解码 XOR-MAPPED-ADDRESS 属性值：1 字节保留 + 1 字节地址族 +
/// 2 字节 x-port + 4/16 字节 x-address（与魔数、事务 ID 异或还原）。
fn decode_xor_address(value: &[u8], transaction_id: &[u8; 12]) -> Option<SocketAddr> {
    let family = *value.get(1)?;
    let x_port = u16::from_be_bytes([*value.get(2)?, *value.get(3)?]);
    let port = x_port ^ u16::from_be_bytes([MAGIC_COOKIE[0], MAGIC_COOKIE[1]]);
    match family {
        0x01 => {
            let bytes: [u8; 4] = value.get(4..8)?.try_into().expect("切片长度已校验");
            let octets = std::array::from_fn(|i| bytes[i] ^ MAGIC_COOKIE[i]);
            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::from(octets)), port))
        }
        0x02 => {
            let bytes: [u8; 16] = value.get(4..20)?.try_into().expect("切片长度已校验");
            let mut key = [0u8; 16];
            key[..4].copy_from_slice(&MAGIC_COOKIE);
            key[4..].copy_from_slice(transaction_id);
            let octets = std::array::from_fn(|i| bytes[i] ^ key[i]);
            Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(octets)), port))
        }
        _ => None,
    }
}

/// 生成随机事务 ID（OS CSPRNG）。
pub fn new_transaction_id() -> [u8; 12] {
    let mut id = [0u8; 12];
    getrandom::fill(&mut id).expect("OS 随机源不可用");
    id
}

/// 对单个 STUN 服务器地址发 Binding Request 并解出公网 UDP 映射地址（薄 IO 壳）。
///
/// 本地套接字按目标地址族绑定通配地址：IPv6 服务器走 IPv6 套接字，
/// 保证 IPv6 候选真的被采集（质量红线）。超时钉在公网探测 8 秒红线；失败为 `None`。
pub async fn query_stun(server: SocketAddr) -> Option<SocketAddr> {
    let bind_addr = match server {
        SocketAddr::V4(_) => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        SocketAddr::V6(_) => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
    };
    let socket = tokio::net::UdpSocket::bind(SocketAddr::new(bind_addr, 0))
        .await
        .ok()?;
    let transaction_id = new_transaction_id();
    socket
        .send_to(&binding_request(transaction_id), server)
        .await
        .ok()?;
    let mut buf = vec![0u8; 1500];
    let (len, _) =
        tokio::time::timeout(crate::net::http::PUBLIC_TIMEOUT, socket.recv_from(&mut buf))
            .await
            .ok()?
            .ok()?;
    parse_xor_mapped_address(&buf[..len], &transaction_id)
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::{
        SocketAddr, binding_request, new_transaction_id, parse_xor_mapped_address, query_stun,
    };

    /// 手算正例（IPv4）：Binding Success Response 携带 1.2.3.4:5678 的
    /// XOR-MAPPED-ADDRESS。x-port = 5678(0x162E) ^ 0x2112 = 0x373C；
    /// x-address = 1.2.3.4 ^ 0x2112A442 = 20 10 A7 46。
    const V4_RESPONSE: &[u8] = &[
        0x01, 0x01, 0x00, 0x0C, // 类型 0x0101，消息长 12
        0x21, 0x12, 0xA4, 0x42, // 魔数
        0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, 0x0f, // 事务 ID
        0x00, 0x20, 0x00, 0x08, // XOR-MAPPED-ADDRESS，长 8
        0x00, 0x01, 0x37, 0x3C, // 保留 0，family 1 (IPv4)，x-port
        0x20, 0x10, 0xA7, 0x46, // x-address
    ];

    /// 手算正例（IPv6）：[2001:db8::1]:3478。事务 ID 全零，x-port = 3478(0x0D96) ^ 0x2112 = 0x2C84；
    /// x-address = 地址 ^ (魔数||事务ID)：前四字节 20 01 0d b8 → 01 13 a9 fa，其余不变。
    const V6_RESPONSE: &[u8] = &[
        0x01, 0x01, 0x00, 0x18, // 类型 0x0101，消息长 24
        0x21, 0x12, 0xA4, 0x42, // 魔数
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // 事务 ID
        0x00, 0x20, 0x00, 0x14, // XOR-MAPPED-ADDRESS，长 20
        0x00, 0x02, 0x2C, 0x84, // 保留 0，family 2 (IPv6)，x-port
        0x01, 0x13, 0xA9, 0xFA, // x-address 前 4 字节
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, // 地址中段 11 个零
        0x01, // 地址末字节 1（与全零事务 ID 异或不变）
    ];

    fn txid(value: u8) -> [u8; 12] {
        [value; 12]
    }

    #[test]
    fn binding_request_layout_matches_rfc5389() {
        let request = binding_request(txid(7));
        assert_eq!(request.len(), 20);
        assert_eq!(&request[0..2], &[0x00, 0x01], "Binding Request 类型 0x0001");
        assert_eq!(&request[2..4], &[0x00, 0x00], "无属性，消息长 0");
        assert_eq!(&request[4..8], &[0x21, 0x12, 0xA4, 0x42], "魔数");
        assert_eq!(&request[8..20], &[7; 12], "事务 ID 原样");
    }

    #[test]
    fn parse_ipv4_xor_mapped_address() {
        let parsed = parse_xor_mapped_address(V4_RESPONSE, &txid(0x0f));
        let expected: SocketAddr = "1.2.3.4:5678".parse().expect("期望地址应合法");
        assert_eq!(parsed, Some(expected));
    }

    #[test]
    fn parse_ipv6_xor_mapped_address() {
        let parsed = parse_xor_mapped_address(V6_RESPONSE, &txid(0));
        let expected: SocketAddr = "[2001:db8::1]:3478".parse().expect("期望地址应合法");
        assert_eq!(parsed, Some(expected));
    }

    #[test]
    fn parse_skips_leading_unknown_attributes() {
        // 在 XOR-MAPPED-ADDRESS 前插一个 SOFTWARE (0x8022) 属性（值 "zt"，4 字节对齐）
        let mut response = V4_RESPONSE[..20].to_vec();
        response.extend_from_slice(&[0x80, 0x22, 0x00, 0x02, b'z', b't', 0x00, 0x00]);
        response.extend_from_slice(&V4_RESPONSE[20..]);
        // 消息长度字段同步 +8
        let mut patched = response;
        patched[3] = 0x0C + 0x08;
        let parsed = parse_xor_mapped_address(&patched, &txid(0x0f));
        let expected: SocketAddr = "1.2.3.4:5678".parse().expect("期望地址应合法");
        assert_eq!(parsed, Some(expected));
    }

    #[test]
    fn malformed_inputs_return_none_without_panic() {
        // 空输入 / 截断头部 / 截断属性
        assert_eq!(parse_xor_mapped_address(&[], &txid(1)), None);
        assert_eq!(
            parse_xor_mapped_address(&V4_RESPONSE[..19], &txid(0x0f)),
            None
        );
        assert_eq!(
            parse_xor_mapped_address(&V4_RESPONSE[..26], &txid(0x0f)),
            None
        );
        // 错误响应类型（0x0111 Binding Error Response）
        let mut error = V4_RESPONSE.to_vec();
        error[1] = 0x11;
        assert_eq!(parse_xor_mapped_address(&error, &txid(0x0f)), None);
        // 魔数不匹配
        let mut bad_cookie = V4_RESPONSE.to_vec();
        bad_cookie[4] = 0x00;
        assert_eq!(parse_xor_mapped_address(&bad_cookie, &txid(0x0f)), None);
        // 事务 ID 不匹配
        assert_eq!(parse_xor_mapped_address(V4_RESPONSE, &txid(1)), None);
        // 缺 XOR-MAPPED-ADDRESS 属性（只有未知属性）
        let mut no_attr = V4_RESPONSE[..20].to_vec();
        no_attr.extend_from_slice(&[0x80, 0x22, 0x00, 0x00]);
        assert_eq!(parse_xor_mapped_address(&no_attr, &txid(0x0f)), None);
        // 属性声明长度超出缓冲
        let mut overlong = V4_RESPONSE.to_vec();
        overlong[21] = 0xFF; // 属性长度低字节改大
        assert_eq!(parse_xor_mapped_address(&overlong, &txid(0x0f)), None);
        // 未知地址族（family 0x03）
        let mut bad_family = V4_RESPONSE.to_vec();
        bad_family[25] = 0x03;
        assert_eq!(parse_xor_mapped_address(&bad_family, &txid(0x0f)), None);
        // 属性长度不足（family IPv4 但值只有 4 字节）
        let mut short_attr = V4_RESPONSE.to_vec();
        short_attr[23] = 0x04; // 属性长度 8 → 4
        assert_eq!(parse_xor_mapped_address(&short_attr, &txid(0x0f)), None);
    }

    #[test]
    fn response_rejects_inconsistent_message_length() {
        for length in [0_u16, 8, 13, 16] {
            let mut response = V4_RESPONSE.to_vec();
            response[2..4].copy_from_slice(&length.to_be_bytes());
            assert_eq!(parse_xor_mapped_address(&response, &txid(0x0f)), None);
        }
    }

    #[test]
    fn transaction_ids_are_random() {
        let a = new_transaction_id();
        let b = new_transaction_id();
        assert_ne!(a, b, "连续两个事务 ID 不应相同");
    }

    /// 手动回环冒烟（不依赖外网）：假 STUN 服务器应答 Binding，
    /// `query_stun` 应解出来源映射地址。响应由测试内独立编码。
    #[tokio::test]
    #[ignore = "需要真实 UDP socket，手动执行回环冒烟"]
    async fn query_stun_parses_response_from_live_socket() {
        let server = tokio::net::UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("绑定回环端口");
        let port = server.local_addr().expect("本地地址").port();
        tokio::spawn(async move {
            let mut buf = [0u8; 64];
            let (len, peer) = server.recv_from(&mut buf).await.expect("收请求");
            // 请求必须是 20 字节 Binding Request，事务 ID 原样回传
            assert_eq!(len, 20);
            assert_eq!(&buf[..2], &[0x00, 0x01]);
            assert_eq!(&buf[2..4], &[0x00, 0x00]);
            assert_eq!(&buf[4..8], &[0x21, 0x12, 0xA4, 0x42]);
            let txid: [u8; 12] = buf[8..20].try_into().expect("事务 ID");
            let response = test_success_response(peer.ip(), peer.port(), &txid);
            server.send_to(&response, peer).await.expect("发响应");
        });

        let mapped = query_stun(format!("127.0.0.1:{port}").parse().expect("回环地址")).await;
        let mapped = mapped.expect("回环应答应解出映射地址");
        assert_eq!(mapped.ip(), IpAddr::from([127, 0, 0, 1]));
        assert!(mapped.port() > 0, "映射端口应是客户端临时端口");
    }

    /// 测试内独立实现的 Binding Success Response 编码（与被测代码互为对照）。
    fn test_success_response(ip: IpAddr, port: u16, txid: &[u8; 12]) -> Vec<u8> {
        let mut response = vec![0x01, 0x01, 0x00, 0x0C];
        response.extend_from_slice(&[0x21, 0x12, 0xA4, 0x42]);
        response.extend_from_slice(txid);
        response.extend_from_slice(&[0x00, 0x20, 0x00, 0x08]);
        response.push(0x00);
        response.push(0x01);
        response.extend_from_slice(&(port ^ 0x2112).to_be_bytes());
        let [a, b, c, d] = match ip {
            IpAddr::V4(v4) => v4.octets(),
            IpAddr::V6(_) => panic!("测试只用 IPv4"),
        };
        response.extend_from_slice(&[a ^ 0x21, b ^ 0x12, c ^ 0xA4, d ^ 0x42]);
        response
    }
}

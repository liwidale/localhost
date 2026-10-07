use std::net::IpAddr;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Listener {
    pub pid: u32,
    pub addr: IpAddr,
    pub port: u16,
}

#[derive(Default)]
pub struct SocketScanner {
    #[cfg(target_os = "linux")]
    owners: std::collections::HashMap<u64, u32>,
}

impl SocketScanner {
    pub fn listening(&mut self) -> Vec<Listener> {
        let mut out = self.scan();
        out.sort_unstable();
        out.dedup();
        out
    }
}

#[cfg(windows)]
impl SocketScanner {
    fn scan(&mut self) -> Vec<Listener> {
        use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6};
        let mut out = windows::table(AF_INET as u32);
        out.extend(windows::table(AF_INET6 as u32));
        out
    }
}

#[cfg(windows)]
mod windows {
    use super::Listener;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
        TCP_TABLE_OWNER_PID_LISTENER,
    };
    use windows_sys::Win32::Networking::WinSock::AF_INET;

    pub fn table(family: u32) -> Vec<Listener> {
        let mut size = 0u32;
        let mut buf: Vec<u64> = Vec::new();
        for _ in 0..4 {
            let ret = unsafe {
                GetExtendedTcpTable(
                    buf.as_mut_ptr().cast(),
                    &mut size,
                    0,
                    family,
                    TCP_TABLE_OWNER_PID_LISTENER,
                    0,
                )
            };
            match ret {
                NO_ERROR => return parse(&buf, family),
                ERROR_INSUFFICIENT_BUFFER => buf = vec![0; (size as usize).div_ceil(8) + 1],
                _ => return Vec::new(),
            }
        }
        Vec::new()
    }

    fn parse(buf: &[u64], family: u32) -> Vec<Listener> {
        let base = buf.as_ptr().cast::<u8>();
        let len = buf.len() * 8;
        if len < 4 {
            return Vec::new();
        }
        let count = unsafe { base.cast::<u32>().read_unaligned() } as usize;
        let rows = unsafe { base.add(4) };
        let mut out = Vec::with_capacity(count);
        if family == AF_INET as u32 {
            let row_size = size_of::<MIB_TCPROW_OWNER_PID>();
            for i in 0..count.min((len - 4) / row_size) {
                let row = unsafe {
                    rows.add(i * row_size)
                        .cast::<MIB_TCPROW_OWNER_PID>()
                        .read_unaligned()
                };
                out.push(Listener {
                    pid: row.dwOwningPid,
                    addr: IpAddr::V4(Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes())),
                    port: port(row.dwLocalPort),
                });
            }
        } else {
            let row_size = size_of::<MIB_TCP6ROW_OWNER_PID>();
            for i in 0..count.min((len - 4) / row_size) {
                let row = unsafe {
                    rows.add(i * row_size)
                        .cast::<MIB_TCP6ROW_OWNER_PID>()
                        .read_unaligned()
                };
                out.push(Listener {
                    pid: row.dwOwningPid,
                    addr: IpAddr::V6(Ipv6Addr::from(row.ucLocalAddr)),
                    port: port(row.dwLocalPort),
                });
            }
        }
        out
    }

    fn port(raw: u32) -> u16 {
        let b = raw.to_ne_bytes();
        u16::from_be_bytes([b[0], b[1]])
    }
}

#[cfg(target_os = "linux")]
impl SocketScanner {
    fn scan(&mut self) -> Vec<Listener> {
        let mut sockets = linux::table("/proc/net/tcp", false);
        sockets.extend(linux::table("/proc/net/tcp6", true));
        if sockets
            .iter()
            .any(|(inode, _, _)| !self.owners.contains_key(inode))
        {
            self.owners = linux::owners();
        }
        sockets
            .into_iter()
            .filter_map(|(inode, addr, port)| {
                let pid = *self.owners.get(&inode)?;
                Some(Listener { pid, addr, port })
            })
            .collect()
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashMap;
    use std::fs;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    pub fn table(path: &str, v6: bool) -> Vec<(u64, IpAddr, u16)> {
        let Ok(text) = fs::read_to_string(path) else {
            return Vec::new();
        };
        text.lines()
            .skip(1)
            .filter_map(|l| parse_line(l, v6))
            .collect()
    }

    pub(super) fn parse_line(line: &str, v6: bool) -> Option<(u64, IpAddr, u16)> {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 10 || cols[3] != "0A" {
            return None;
        }
        let (addr, port) = cols[1].split_once(':')?;
        let port = u16::from_str_radix(port, 16).ok()?;
        let addr = if v6 {
            if addr.len() != 32 {
                return None;
            }
            let mut bytes = [0u8; 16];
            for i in 0..4 {
                let word = u32::from_str_radix(&addr[i * 8..i * 8 + 8], 16).ok()?;
                bytes[i * 4..i * 4 + 4].copy_from_slice(&word.to_ne_bytes());
            }
            IpAddr::V6(Ipv6Addr::from(bytes))
        } else {
            let word = u32::from_str_radix(addr, 16).ok()?;
            IpAddr::V4(Ipv4Addr::from(word.to_ne_bytes()))
        };
        let inode = cols[9].parse().ok()?;
        Some((inode, addr, port))
    }

    pub fn owners() -> HashMap<u64, u32> {
        let mut map = HashMap::new();
        let Ok(entries) = fs::read_dir("/proc") else {
            return map;
        };
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(fds) = fs::read_dir(entry.path().join("fd")) else {
                continue;
            };
            for fd in fds.flatten() {
                if let Ok(target) = fs::read_link(fd.path())
                    && let Some(inode) = target
                        .to_str()
                        .and_then(|t| t.strip_prefix("socket:["))
                        .and_then(|t| t.strip_suffix(']'))
                        .and_then(|t| t.parse().ok())
                {
                    map.insert(inode, pid);
                }
            }
        }
        map
    }
}

#[cfg(target_os = "macos")]
impl SocketScanner {
    fn scan(&mut self) -> Vec<Listener> {
        macos::scan()
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::Listener;
    use libproc::libproc::bsd_info::BSDInfo;
    use libproc::libproc::file_info::{ListFDs, ProcFDType, pidfdinfo};
    use libproc::libproc::net_info::{SocketFDInfo, SocketInfoKind};
    use libproc::libproc::proc_pid::{listpidinfo, pidinfo};
    use libproc::processes::{ProcFilter, pids_by_type};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    const TCP_LISTEN: i32 = 1;
    const INI_IPV4: u8 = 0x1;

    pub fn scan() -> Vec<Listener> {
        let Ok(pids) = pids_by_type(ProcFilter::All) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for pid in pids {
            let Ok(info) = pidinfo::<BSDInfo>(pid as i32, 0) else {
                continue;
            };
            let Ok(fds) = listpidinfo::<ListFDs>(pid as i32, info.pbi_nfiles as usize) else {
                continue;
            };
            for fd in fds {
                if !matches!(fd.proc_fdtype.into(), ProcFDType::Socket) {
                    continue;
                }
                let Ok(socket) = pidfdinfo::<SocketFDInfo>(pid as i32, fd.proc_fd) else {
                    continue;
                };
                if !matches!(socket.psi.soi_kind.into(), SocketInfoKind::Tcp) {
                    continue;
                }
                let tcp = unsafe { socket.psi.soi_proto.pri_tcp };
                if tcp.tcpsi_state != TCP_LISTEN {
                    continue;
                }
                let ini = tcp.tcpsi_ini;
                let port = u16::from_be(ini.insi_lport as u16);
                let addr = if ini.insi_vflag & INI_IPV4 != 0 {
                    let raw = unsafe { ini.insi_laddr.ina_46.i46a_addr4.s_addr };
                    IpAddr::V4(Ipv4Addr::from(raw.to_ne_bytes()))
                } else {
                    let raw = unsafe { ini.insi_laddr.ina_6.s6_addr };
                    IpAddr::V6(Ipv6Addr::from(raw))
                };
                out.push(Listener { pid, addr, port });
            }
        }
        out
    }
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
impl SocketScanner {
    fn scan(&mut self) -> Vec<Listener> {
        Vec::new()
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::linux::parse_line;
    use std::net::IpAddr;

    #[test]
    fn parses_listening_v4() {
        let line = "   0: 0100007F:0BB8 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 41234 1 0000000000000000 100 0 0 10 0";
        let (inode, addr, port) = parse_line(line, false).unwrap();
        assert_eq!(inode, 41234);
        assert_eq!(addr, "127.0.0.1".parse::<IpAddr>().unwrap());
        assert_eq!(port, 3000);
    }
}

#[cfg(all(test, any(windows, target_os = "linux", target_os = "macos")))]
mod live {
    use super::SocketScanner;

    #[test]
    fn finds_own_listener() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let found = SocketScanner::default().listening();
        assert!(
            found
                .iter()
                .any(|l| l.port == port && l.pid == std::process::id())
        );
    }
}

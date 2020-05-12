use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use pnet::datalink::{MacAddr};
use std::convert::From;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub trait ToMac {
    fn to_mac(&self) -> MacAddr;
}

impl ToMac for u64 {
    fn to_mac(&self) -> MacAddr {
        let mut b = self.to_be_bytes();
        b[2] |= 0b0000_0010; // set local managed bit
        MacAddr::new(b[2], b[3], b[4], b[5], b[6], b[7])
    }
}

impl ToMac for Ipv4Addr {
    fn to_mac(&self) -> MacAddr {
        let o = self.octets();
        MacAddr::new(0b0000_0010, 0, o[0], o[1], o[2], o[3])
    }
}

pub trait TryToMac {
    fn try_to_mac(&self) -> Option<MacAddr>;
}

impl<T: ToMac> TryToMac for T {
    fn try_to_mac(&self) -> Option<MacAddr> {
        Some(self.to_mac())
    }
}

impl TryToMac for Ipv6Addr {
    fn try_to_mac(&self) -> Option<MacAddr> {
        self.to_eu64_mac()
            .or_else(|| self.to_ipv4().map(|v4| v4.to_mac()))
    }
}

impl TryToMac for IpAddr {
    fn try_to_mac(&self) -> Option<MacAddr> {
        match self {
            Self::V4(v4) => v4.try_to_mac(),
            Self::V6(v6) => v6.try_to_mac(),
        }
    }
}

pub trait ToIpv6 {
    fn to_ipv6(&self) -> Ipv6Addr;
}

impl ToIpv6 for u64 {
    fn to_ipv6(&self) -> Ipv6Addr {
        let b = self.to_be_bytes();
        std::convert::From::from([
            0, 0, 0, 0, 0, 0, 0, 0, b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ])
    }
}

pub trait ToIpv4 {
    fn to_ipv4(&self) -> Ipv4Addr;
}

impl ToIpv4 for u64 {
    fn to_ipv4(&self) -> Ipv4Addr {
        let b = self.to_be_bytes();
        Ipv4Addr::new(b[4], b[5], b[6], b[7])
    }
}

impl ToIpv4 for MacAddr {
    fn to_ipv4(&self) -> Ipv4Addr {
        Ipv4Addr::new(self.2, self.3, self.4, self.5)
    }
}

pub trait ToEUI64Ipv6Addr {
    fn to_eui64_ipv6(&self) -> Ipv6Addr;
}

impl ToEUI64Ipv6Addr for MacAddr {
    fn to_eui64_ipv6(&self) -> Ipv6Addr {
        std::convert::From::from([
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            self.0 ^ 0b0000_0010, // flip local managed bit
            self.1,
            self.2,
            0xff,
            0xfe,
            self.3,
            self.4,
            self.5,
        ])
    }
}

pub trait ToEUI64Mac {
    fn to_eu64_mac(&self) -> Option<MacAddr>;
}

impl ToEUI64Mac for Ipv6Addr {
    fn to_eu64_mac(&self) -> Option<MacAddr> {
        let o = self.octets();
        if o[11] != 0xff || o[12] != 0xfe {
            None
        } else {
            Some(MacAddr::new(
                o[8] ^ 0b0000_0010, // flip local managed bit
                o[9],
                o[10],
                o[13],
                o[14],
                o[15],
            ))
        }
    }
}

pub trait InNet<N, A> {
    fn in_net(&self, net: &N) -> A;
}

impl InNet<Ipv6Network, Ipv6Addr> for Ipv6Addr {
    fn in_net(&self, net: &Ipv6Network) -> Self {
        int_in_net(
            u128::from(*self),
            u128::from(net.network()),
            u128::from(net.mask()),
        )
        .into()
    }
}

impl InNet<Ipv4Network, Ipv4Addr> for Ipv4Addr {
    fn in_net(&self, net: &Ipv4Network) -> Self {
        int_in_net(
            u32::from(*self),
            u32::from(net.network()),
            u32::from(net.mask()),
        )
        .into()
    }
}

impl InNet<Ipv6Network, Ipv6Addr> for Ipv4Addr {
    fn in_net(&self, net: &Ipv6Network) -> Ipv6Addr {
        self.to_ipv6_compatible().in_net(net)
    }
}

pub trait TryInNet<N, A> {
    fn try_in_net(&self, net: &N) -> Option<A>;
}

impl<N, A, S: InNet<N, A>> TryInNet<N, A> for S {
    fn try_in_net(&self, net: &N) -> Option<A> {
        Some(self.in_net(net))
    }
}

impl TryInNet<Ipv4Network, Ipv4Addr> for Ipv6Addr {
    fn try_in_net(&self, net: &Ipv4Network) -> Option<Ipv4Addr> {
        self.to_ipv4().map(|v4| v4.in_net(net))
    }
}

impl TryInNet<IpNetwork, IpAddr> for Ipv4Addr {
    fn try_in_net(&self, net: &IpNetwork) -> Option<IpAddr> {
        match net {
            IpNetwork::V4(v4) => self.try_in_net(v4).map(|v4| IpAddr::V4(v4)),
            IpNetwork::V6(v6) => self.try_in_net(v6).map(|v6| IpAddr::V6(v6)),
        }
    }
}

impl TryInNet<IpNetwork, IpAddr> for Ipv6Addr {
    fn try_in_net(&self, net: &IpNetwork) -> Option<IpAddr> {
        match net {
            IpNetwork::V4(v4) => self.try_in_net(v4).map(|v4| IpAddr::V4(v4)),
            IpNetwork::V6(v6) => self.try_in_net(v6).map(|v6| IpAddr::V6(v6)),
        }
    }
}

impl TryInNet<IpNetwork, IpAddr> for IpAddr {
    fn try_in_net(&self, net: &IpNetwork) -> Option<IpAddr> {
        match self {
            IpAddr::V4(v4) => v4.try_in_net(net),
            IpAddr::V6(v6) => v6.try_in_net(net),
        }
    }
}

impl InNet<IpNetwork, IpAddr> for MacAddr {
    fn in_net(&self, net: &IpNetwork) -> IpAddr {
        match net {
            IpNetwork::V6(v6net) => IpAddr::V6(self.to_eui64_ipv6().in_net(v6net)),
            IpNetwork::V4(v4net) => IpAddr::V4(self.to_ipv4().in_net(v4net)),
        }
    }
}

impl InNet<IpNetwork, IpAddr> for u64 {
    fn in_net(&self, net: &IpNetwork) -> IpAddr {
        match net {
            IpNetwork::V6(v6net) => IpAddr::V6(self.to_ipv6().in_net(v6net)),
            IpNetwork::V4(v4net) => IpAddr::V4(self.to_ipv4().in_net(v4net)),
        }
    }
}

fn int_in_net<
    I: Clone + std::ops::BitAnd<Output = I> + std::ops::Not<Output = I> + std::ops::BitOr<Output = I>,
>(
    ip: I,
    net: I,
    mask: I,
) -> I {
    (net & mask.clone()) | (ip & !mask)
}
use ipnetwork::{IpNetwork, Ipv4Network, Ipv6Network};
use pnet::datalink;
use pnet::datalink::{interfaces, MacAddr, NetworkInterface};
use std::convert::From;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub trait FromInt<I: std::convert::Into<u64>> {
    fn from(i: I) -> Self;
}

impl<I: std::convert::Into<u64>> FromInt<I> for MacAddr {
    fn from(i: I) -> Self {
        let mut b = i.into().to_be_bytes();
        b[2] |= 0b0000_0010; // set local managed bit
        MacAddr::new(b[2], b[3], b[4], b[5], b[6], b[7])
    }
}

impl<I: std::convert::Into<u64>> FromInt<I> for Ipv4Addr {
    fn from(i: I) -> Self {
        let b = i.into().to_be_bytes();
        Ipv4Addr::new(b[4], b[5], b[6], b[7])
    }
}

impl<I: std::convert::Into<u64>> FromInt<I> for Ipv6Addr {
    fn from(i: I) -> Self {
        let b = i.into().to_be_bytes();
        std::convert::From::from([
            0, 0, 0, 0, 0, 0, 0, 0, b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ])
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

pub trait IntoNet<N, A> {
    fn into_net(self, net: N) -> A;
}

impl IntoNet<Ipv6Network, Ipv6Addr> for Ipv6Addr {
    fn into_net(self, net: Ipv6Network) -> Self {
        int_in_net(
            u128::from(self),
            u128::from(net.network()),
            u128::from(net.mask()),
        )
        .into()
    }
}

impl IntoNet<Ipv4Network, Ipv4Addr> for Ipv4Addr {
    fn into_net(self, net: Ipv4Network) -> Self {
        int_in_net(
            u32::from(self),
            u32::from(net.network()),
            u32::from(net.mask()),
        )
        .into()
    }
}

impl IntoNet<Ipv6Network, Ipv6Addr> for Ipv4Addr {
    fn into_net(self, net: Ipv6Network) -> Ipv6Addr {
        self.to_ipv6_compatible().into_net(net)
    }
}

pub trait TryIntoNet<N, A> {
    fn try_into_net(self, net: N) -> Option<A> where A: std::marker::Sized ;
}

impl<N, A, S: IntoNet<N, A>> TryIntoNet<N, A> for S {
    fn try_into_net(self, net: N) -> Option<A> {
        Some(self.into_net(net))
    }
}

impl TryIntoNet<Ipv4Network, Ipv4Addr> for Ipv6Addr {
    fn try_into_net(self, net: Ipv4Network) -> Option<Ipv4Addr> {
        self.to_ipv4().map(|v4| v4.into_net(net))
    }
}

impl TryIntoNet<IpNetwork, IpAddr> for Ipv4Addr {
    fn try_into_net(self, net: IpNetwork) -> Option<IpAddr> {
        match net {
            IpNetwork::V4(v4) => self.try_into_net(v4).map(|v4| IpAddr::V4(v4)),
            IpNetwork::V6(v6) => self.try_into_net(v6).map(|v6| IpAddr::V6(v6)),
        }
    }
}

impl TryIntoNet<IpNetwork, IpAddr> for Ipv6Addr {
    fn try_into_net(self, net: IpNetwork) -> Option<IpAddr> {
        match net {
            IpNetwork::V4(v4) => self.try_into_net(v4).map(|v4| IpAddr::V4(v4)),
            IpNetwork::V6(v6) => self.try_into_net(v6).map(|v6| IpAddr::V6(v6)),
        }
    }
}

impl TryIntoNet<IpNetwork, IpAddr> for IpAddr {
    fn try_into_net(self, net: IpNetwork) -> Option<IpAddr> {
        match self {
            IpAddr::V4(v4) => v4.try_into_net(net),
            IpAddr::V6(v6) => v6.try_into_net(net),
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

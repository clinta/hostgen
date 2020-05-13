I have a rather large home lab, with lots of servers and containers using lots of IP addresses. I run a combination of dnsmasq and nsd. And I run a dual-stack network with a dynamically generated ipv6 prefix. I wanted an easy way to manage DNS records for my servers and containers and this is what I came up with. This little program generates dnsmasq host entries (dhcp reservations) and zone records from a simple yaml configuration.

The yaml looks something like this:

```yaml
eth0: # an interface on the router
  server1: # the server hostname
    - 10 # an integer for the IP address and mac address
```

Now suppose for these examples that eth0 had the addresses `192.168.1.1/24` and `2001:DB8::/64`.

You can also select interfaces using glob patterns, or as a list of selectors:

```yaml
["eth0", "eth1"]:
```

Or you cans elect networks.

```yaml
192.168.0.0/16:
```

This tells the program to generate IP addresses for subnets inside the `192.168.0.0/16` network, it does not generate IP addresses using the `192.168.0.0/16` network directly.
Instead it selects local networks that are inside that network. So in the example where eth0 has the address `192.168.0.1/24` on that network, this
configuration will generate addresses in the `192.168.0.0/24` network.

Since the only configuration listed for this server is a single integer, first the program will synthisize a mac address from it.
In this case the mac address will be `02:00:00:00:00:0a`. Where does the `02` come from? Well the script is assuming that this is a locally managed mac address
rather than a universal mac address assigned by the manufacturer. So bit 7 in the mac address is set to `1`. I'm sure I don't have to explain that the `0a` is hex for `10`.

If you want to speficy your own mac address, this is easy, just add it to the list in the yaml:

```yaml
eth0:
  server1:
    - 10
    - "02:0A:0B:0C:0D:0E"
```

Now with a mac address set, the program will choose an IP address for each network on the interface, starting with `192.168.1.1`. This is pretty straightforward, the address will be `192.168.1.10`. What's happening under the hood is the address `10` (`0.0.0.10`)  is masked to the host bits of the network address. This means if you choose an integer that is larger than the maximum number of hosts in your network, the results will wrap. So `267` applied to `192.168.1.0/24` will result in `192.168.1.11`.

If you want to specify your own IPv4 address, again this is easy, just put something that looks like an IPv4 address in the yaml. Just remember, only the host bits matter. The below example will result in an ip of `192.168.1.5`.

```yaml
eth0:
  server1:
    - 10
    - 0.0.0.5
```

IPv6 addresses are generated using EUI-64 rules with the mac address. In this case the address will be `2001:db8::ff:fe00:a`.

Just like IPv4 addresses, you can put something that looks like an IPv6 address into the yaml to override it. And again, only the host bits matter. The below example will generate the address `2001:db8::5`.

```yaml
eth0:
  server1:
    - 10
    - "::5"
```

You can customize the generation of ipv4, or ipv6 adddresses using tags as
follows:

```yaml
eth0:
  server1:
    - 10
    - ip6: 5
    - mac: 40
```

You can skip generating any specific item using `Null`

```yaml
eth0:
  server1:
    - 10
    - ip6: Null
```

Once you have your yaml configuration build, generating the dnsmasq or zone entries is easy. Just run

`hostgen -c hosts.yaml dnsmasq` or `hostgen -c hosts.yaml zone`

I designed this the way it is to meet my own needs on my own lab network, which may explain some of the design decisions. If you find this useful and have ideas of how to make it more useful or generic to fit more usecases, I welcome any discussion or contributions.

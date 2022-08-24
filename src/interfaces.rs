/*
 * Copyright [2022] <mcr@sandelman.ca>

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
 *
 */

// this module listens for interfaces and then sorts them into three lists
// according to the provided list of acp interfaces, downlink interfaces, and
// interfacs to ignore.
// The lists may include glob(1) wildcards.
//
// Interfaces which match none of the lists are placed into the downlink interface
// list if the list is empty, otherwise, they are ignored
//

use std::net::Ipv6Addr;
use std::collections::HashMap;
use std::sync::Arc;
use futures::lock::Mutex;
use futures::stream::StreamExt;

use rtnetlink::{
    constants::{RTMGRP_IPV6_ROUTE, RTMGRP_IPV6_IFADDR, RTMGRP_LINK},
    Error,
    new_connection,
    sys::{AsyncSocket, SocketAddr},
};
use netlink_packet_route::{
    NetlinkPayload::InnerMessage,
    RtnlMessage::NewLink,
    RtnlMessage::DelLink,
    RtnlMessage::NewAddress,
    RtnlMessage::NewRoute,
    RtnlMessage::DelRoute,
    RtnlMessage::DelAddress,
    LinkMessage, AddressMessage
};
use netlink_packet_route::link::nlas::AfSpecInet;
use netlink_packet_route::link::nlas::State;

use crate::debugoptions::DebugOptions;
use crate::args::RoosterOptions;

pub type IfIndex = u32;

pub struct Interface {
    pub ifindex:       IfIndex,
    pub ifname:        String,
    pub ignored:       bool,
    pub mtu:           u32,
    pub linklocal6:    Ipv6Addr,
    pub oper_state:    State
    //pub otherstuff:    Box<>
}

impl Interface {
    pub fn empty(ifi: IfIndex) -> Interface {
        Interface {
            ifindex: ifi,
            ifname:  "".to_string(),
            ignored: false,
            mtu:     0,
            linklocal6: Ipv6Addr::UNSPECIFIED,
            oper_state: State::Down
        }
    }
}

pub struct AllInterfaces {
    pub debug:           DebugOptions,
    pub interfaces:      HashMap<u32, Arc<Mutex<Interface>>>,
    pub acp_interfaces:  HashMap<u32, Arc<Mutex<Interface>>>,
    pub downlink_interfaces:  HashMap<u32, Arc<Mutex<Interface>>>
}

impl AllInterfaces {
    pub fn default() -> AllInterfaces {
        return AllInterfaces {
            debug:      DebugOptions::default(),
            interfaces: HashMap::new(),
            acp_interfaces: HashMap::new(),
            downlink_interfaces: HashMap::new()
        }
    }

    pub async fn get_entry_by_ifindex<'a>(self: &'a mut AllInterfaces, ifindex: IfIndex) -> Arc<Mutex<Interface>> {
        let ifnl = self.interfaces.entry(ifindex).or_insert_with(|| { Arc::new(Mutex::new(Interface::empty(ifindex)))});
        return ifnl.clone();
    }

    // not really sure what this can really return
    pub async fn store_addr_info<'a>(self: &'a mut AllInterfaces, am: AddressMessage) {
        let mut mydebug = self.debug.clone();
        let lh = am.header;
        let ifindex = lh.index;

        mydebug.debug_info(format!("ifindex: {} family: {}", ifindex, lh.family)).await;

        let     ifna = self.get_entry_by_ifindex(ifindex).await;
        let mut ifn  = ifna.lock().await;

        for nlas in am.nlas {
            use netlink_packet_route::address::Nla;
            match nlas {
                Nla::Address(addrset) => {
                    if addrset.len() != 16 {
                        continue;
                    }
                    let mut addrbytes: [u8; 16] = [0; 16];
                    for n in 0..=15 {
                        addrbytes[n] = addrset[n]
                    }
                    let llv6 = Ipv6Addr::from(addrbytes);
                    if llv6.segments()[0] != 0xfe80 {
                        continue;
                    }
                    ifn.linklocal6 = llv6;
                    mydebug.debug_info(format!("llv6: {}", ifn.linklocal6)).await;
                },
                Nla::CacheInfo(_info) => { /* nothing */},
                Nla::Flags(_info)     => { /* nothing */},
                _ => {
                    mydebug.debug_info(format!("data: {:?} ", nlas)).await;
                }
            }
        }
        mydebug.debug_info(format!("")).await;
    }

    pub async fn store_link_info<'a>(self: &'a mut AllInterfaces, lm: LinkMessage) {
        let mut mydebug = self.debug.clone();
        let lh = lm.header;
        let ifindex = lh.index;

        let _results = {
            let     ifna = self.get_entry_by_ifindex(ifindex).await;
            let mut ifn  = ifna.lock().await;

            for nlas in lm.nlas {
                use netlink_packet_route::link::nlas::Nla;
                match nlas {
                    Nla::IfName(name) => {
                        mydebug.debug_info(format!("ifname: {}", name)).await;
                        ifn.ifname = name;
                    },
                    Nla::Mtu(bytes) => {
                        mydebug.debug_info(format!("mtu: {}", bytes)).await;
                        ifn.mtu = bytes;
                    },
                    Nla::Address(addrset) => {
                        mydebug.debug_info(format!("lladdr: {:0x}:{:0x}:{:0x}:{:0x}:{:0x}:{:0x}", addrset[0], addrset[1], addrset[2], addrset[3], addrset[4], addrset[5])).await;
                    },
                    Nla::OperState(state) => {
                        if state == State::Up {
                            mydebug.debug_info(format!("device is up")).await;
                        }
                        ifn.oper_state = state;
                    },
                    Nla::AfSpecInet(inets) => {
                        for ip in inets {
                            match ip {
                                AfSpecInet::Inet(_v4) => { },
                                AfSpecInet::Inet6(_v6) => {
                                    //mydebug.debug_info(format!("v6: {:?}", v6)).await;
                                }
                                _ => {}
                            }
                        }
                    },
                    _ => {
                        //print!("data: {:?} ", nlas);
                    }
                }
            }
            mydebug.debug_info(format!("")).await;
            (ifn.oper_state == State::Down, ifn.ifindex.clone(), ifn.ifname.clone())
        };

        return ();
    }

    pub async fn gather_addr_info(lallif: &Arc<Mutex<AllInterfaces>>, am: AddressMessage) -> Result<(), Error> {
        let mut allif   = lallif.lock().await;
        let _mydebug = allif.debug.clone();

        allif.store_addr_info(am).await;
        Ok(())
    }

    pub async fn gather_link_info(lallif: &Arc<Mutex<AllInterfaces>>, lm: LinkMessage) -> Result<(), Error> {
        let mut allif   = lallif.lock().await;
        let _mydebug = allif.debug.clone();

        allif.store_link_info(lm).await;
        Ok(())
    }

    pub async fn listen_network(lallif: &Arc<Mutex<AllInterfaces>>,
                                _options: &RoosterOptions) ->
        Result<tokio::task::JoinHandle<Result<(),Error>>, String>
    {
        let myif = lallif.clone();
        let listenhandle = tokio::spawn(async move {
            println!("listening to network");

            // Open the netlink socket
            let (mut connection, _handle, mut messages) = new_connection().map_err(|e| format!("{}", e)).unwrap();

            // These flags specify what kinds of broadcast messages we want to listen for.
            let mgroup_flags = RTMGRP_IPV6_ROUTE | RTMGRP_IPV6_IFADDR | RTMGRP_LINK;

            // A netlink socket address is created with said flags.
            let addr = SocketAddr::new(0, mgroup_flags);

            // Said address is bound so new connections and
            // thus new message broadcasts can be received.
            connection.socket_mut().socket_mut().bind(&addr).expect("failed to bind");

            let mut debug = {
                let allif   = myif.lock().await;
                allif.debug.clone()
            };

            tokio::spawn(connection);

            while let Some((message, _)) = messages.next().await {
                let payload = message.payload;
                match payload {
                    InnerMessage(DelRoute(_stuff)) => {
                        /* happens when acp_001 is moved to another namespace */
                        /* need to sort out when it is relevant */
                    }
                    InnerMessage(DelAddress(_stuff)) => {
                        /* happens when acp_001 is moved to another namespace */
                        /* need to sort out when it is relevant by looking at name and LinkHeader */
                    }
                    InnerMessage(DelLink(_stuff)) => {
                        /* happens when acp_001 is moved to another namespace */
                        /* need to sort out when it is relevant by looking at name and LinkHeader */
                    }
                    InnerMessage(NewLink(stuff)) => {
                        AllInterfaces::gather_link_info(&myif,
                                                        stuff).await.unwrap();
                    }
                    InnerMessage(NewAddress(stuff)) => {
                        let _sifn = AllInterfaces::gather_addr_info(&myif,
                                                                    stuff).await.unwrap();
                    }
                    InnerMessage(NewRoute(_thing)) => {
                        /* just ignore these! */
                    }
                    //_ => { println!("generic message type: {} skipped", payload.message_type()); }
                    _ => {
                        debug.debug_info(format!("msg type: {:?}", payload)).await;
                    }
                }
            };
            Ok(())
        });
        Ok(listenhandle)
    }

}

#[cfg(test)]
pub mod tests {
    use super::*;
    use netlink_packet_route::ARPHRD_ETHER;
    use netlink_packet_route::IFF_UP;
    use netlink_packet_route::IFF_LOWER_UP;
    use netlink_packet_route::{
        LinkHeader, AddressHeader,
        AF_INET6
    };

    #[allow(unused_macros)]
    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    fn setup_ai() -> (Arc<Mutex<Vec<u8>>>, AllInterfaces) {
        let writer: Vec<u8> = vec![];
        let awriter = Arc::new(Mutex::new(writer));
        let db1 = DebugOptions { debug_interfaces: true,
                                 debug_output: awriter.clone() };
        let mut all1 = AllInterfaces::default();
        all1.debug = db1;

        (awriter, all1)
    }

    /* define a new interface with ifindex and a Link-Local address */
    fn setup_am() -> AddressMessage {
        use netlink_packet_route::address::nlas::Nla;

        AddressMessage {
            header: AddressHeader { family: AF_INET6 as u8,
                                    prefix_len: 64,
                                    flags: 0,
                                    scope: 0,
                                    index: 10
            },
            nlas: vec![
                Nla::Address(vec![0xfe, 0x80, 0,0, 0,0,0,0,
                                  0x00, 0x00, 0,0, 0,0,0,1])
            ],
        }
    }


    async fn async_add_interface(allif: &mut AllInterfaces) -> Result<(), std::io::Error> {
        assert_eq!(allif.interfaces.len(), 0);
        allif.store_addr_info(setup_am()).await;
        assert_eq!(allif.interfaces.len(), 1);
        Ok(())
    }

    #[test]
    fn test_add_interface() -> Result<(), std::io::Error> {
        let (_awriter, mut all1) = setup_ai();
        aw!(async_add_interface(&mut all1)).unwrap();
        Ok(())
    }

    async fn async_locked_add_interface(lallif: &mut Arc<Mutex<AllInterfaces>>) -> Result<(), std::io::Error> {
        {
            let allif      = lallif.lock().await;
            assert_eq!(allif.interfaces.len(), 0);
        }
        AllInterfaces::gather_addr_info(lallif, setup_am()).await.unwrap();
        {
            let allif      = lallif.lock().await;
            assert_eq!(allif.interfaces.len(), 1);
        }
        Ok(())
    }

    #[test]
    fn test_locked_add_interface() -> Result<(), std::io::Error> {
        let (_awriter, all1) = setup_ai();
        let mut lallif = Arc::new(Mutex::new(all1));
        aw!(async_locked_add_interface(&mut lallif)).unwrap();
        Ok(())
    }

    /* define a new interface with ifindex and a Link-Local address */
    fn setup_lm() -> LinkMessage {
        use netlink_packet_route::link::nlas::Nla;

        LinkMessage {
            header: LinkHeader { interface_family: AF_INET6 as u8,
                                 index: 10,
                                 link_layer_type: ARPHRD_ETHER,
                                 flags: IFF_UP|IFF_LOWER_UP,
                                 change_mask: 0xffff_ffff
            },
            nlas: vec![
                Nla::IfName("eth0".to_string()),
                Nla::Mtu(1500),
                Nla::Address(vec![0x52, 0x54, 0x00, 0x99, 0x9b, 0xba])
            ],
        }
    }

    async fn async_locked_add_link(lallif: &mut Arc<Mutex<AllInterfaces>>) -> Result<(), std::io::Error> {
        {
            let allif      = lallif.lock().await;
            assert_eq!(allif.interfaces.len(), 0);
        }
        AllInterfaces::gather_link_info(lallif, setup_lm()).await.unwrap();
        {
            let allif      = lallif.lock().await;
            assert_eq!(allif.interfaces.len(), 1);
        }
        Ok(())
    }

    #[test]
    fn test_locked_add_link() -> Result<(), std::io::Error> {
        let (_awriter, all1) = setup_ai();
        let mut lallif = Arc::new(Mutex::new(all1));
        aw!(async_locked_add_link(&mut lallif)).unwrap();
        Ok(())
    }

    async fn async_search_entry(allif: &mut AllInterfaces) -> Result<(), std::io::Error> {
        let lifind01 = allif.get_entry_by_ifindex(1).await;
        {
            let mut ifind01 = lifind01.lock().await;
            assert_eq!(ifind01.ifindex, 1);
            ifind01.ifname = "eth0".to_string();
        }

        let lifind02 = allif.get_entry_by_ifindex(2).await;
        {
            let mut ifind02 = lifind02.lock().await;
            assert_eq!(ifind02.ifindex, 2);
            ifind02.ifname = "eth0".to_string();   // yes, same name
        }

        /* retrieve it and see that it kept the data */
        let lifind03 = allif.get_entry_by_ifindex(1).await;
        {
            let ifind03 = lifind03.lock().await;
            assert_eq!(ifind03.ifname, "eth0".to_string());
        }

        assert_eq!(allif.interfaces.len(), 2);
        Ok(())
    }

    #[test]
    fn test_search_entry() -> Result<(), std::io::Error> {
        let (_awriter, mut all1) = setup_ai();
        aw!(async_search_entry(&mut all1)).unwrap();
        Ok(())
    }



}

/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && RUSTFLAGS='-A dead_code -Awarnings' cargo build"
 * End:
 */

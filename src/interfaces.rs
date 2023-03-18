/*
 * Copyright [2022,2023] <mcr@sandelman.ca>

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
// according to the provided list of acp interfaces, joinlink interfaces, and
// interfacs to ignore.
// The lists may include glob(1) wildcards.
//
// Interfaces which match none of the lists are placed into the joinlink interface
// list if the list is empty, otherwise, they are ignored
//

use std::net::{Ipv6Addr, IpAddr, SocketAddr};
use std::collections::HashMap;
use std::sync::Arc;
//use chrono;
use futures::lock::Mutex;
use futures::stream::StreamExt;
use futures::TryStreamExt;

use rtnetlink::{
    constants::{RTMGRP_IPV6_ROUTE, RTMGRP_IPV6_IFADDR, RTMGRP_LINK},
    Handle,
    Error,
    new_connection,
    sys::{AsyncSocket}
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
use crate::interface::Interface;
use crate::interface::IfIndex;
use crate::acp_interface::{RegistrarType};

#[derive(Clone, Copy, Debug)]
pub struct ProxiesEnabled {
    pub http_avail:      bool,
    pub stateful_avail:  bool,  // CoAPS
    pub stateless_avail: bool,  // CoAPS + JPY
}

impl ProxiesEnabled {
    pub fn default() -> ProxiesEnabled {
        return ProxiesEnabled {
            http_avail: false,
            stateful_avail: false,
            stateless_avail: false
        }
    }
}


pub struct AllInterfaces {
    pub debug:           Arc<DebugOptions>,
    pub exitnow:         bool,
    pub invalidate_avail:Arc<Mutex<bool>>,
    pub proxies:         ProxiesEnabled,
    pub interfaces:      HashMap<IfIndex, Arc<Mutex<Interface>>>,
    pub acp_interfaces:  HashMap<IfIndex, Arc<Mutex<Interface>>>,
    // this needs a mutex on it so that the announce iteration loop
    // can run without taking a lock on AllInterfaces.
    pub joinlink_interfaces:  Arc<Mutex<HashMap<IfIndex, Arc<Mutex<Interface>>>>>
}

fn sockaddr_from_addr(n1: IpAddr, port: u16) -> SocketAddr {
    match n1 {
        IpAddr::V4(v4addr) => { SocketAddr::new(IpAddr::V4(v4addr), port) },
        IpAddr::V6(v6addr) => { SocketAddr::new(IpAddr::V6(v6addr), port) },
    }
}

impl AllInterfaces {
    pub fn default() -> AllInterfaces {
        return AllInterfaces {
            debug:      Arc::new(DebugOptions::default()),
            invalidate_avail: Arc::new(Mutex::new(true)),
            exitnow:    false,
            proxies:    ProxiesEnabled::default(),
            interfaces: HashMap::new(),
            acp_interfaces: HashMap::new(),
            joinlink_interfaces: Arc::new(Mutex::new(HashMap::new()))
        }
    }

    pub async fn get_entry_by_ifindex<'a>(self: &'a mut AllInterfaces, ifindex: IfIndex) -> Arc<Mutex<Interface>> {
        let ifnl = self.interfaces.entry(ifindex).or_insert_with(|| {
            Arc::new(Mutex::new(Interface::empty(ifindex, self.debug.clone())))
        });
        return ifnl.clone();
    }

    // not really sure what this can really return
    pub async fn store_addr_info<'a>(self: &'a mut AllInterfaces,
                                     _options: &RoosterOptions,
                                     am: AddressMessage) {
        let mydebug = self.debug.clone();
        let lh = am.header;
        let ifindex = lh.index;

        mydebug.debug_interfaces(format!("ifindex: {} family: {}", ifindex, lh.family)).await;

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
                    mydebug.debug_interfaces(format!("llv6: {}", ifn.linklocal6)).await;
                },
                Nla::CacheInfo(_info) => { /* nothing */},
                Nla::Flags(_info)     => { /* nothing */},
                _ => {
                    mydebug.debug_interfaces(format!("data: {:?} ", nlas)).await;
                }
            }
        }
        mydebug.debug_interfaces(format!("")).await;
    }

    pub async fn store_link_info<'a>(self: &'a mut AllInterfaces,
                                     allif: Arc<Mutex<AllInterfaces>>,
                                     options: &RoosterOptions,
                                     mydebug: Arc<DebugOptions>,
                                     lm: LinkMessage) {
        let lh = lm.header;
        let ifindex = lh.index;

        let (old_oper_state, new_oper_state, ifname)  = {
            let     ifna = self.get_entry_by_ifindex(ifindex).await;
            let mut ifn  = ifna.lock().await;

            let old_oper_state = ifn.oper_state;

            for nlas in lm.nlas {
                use netlink_packet_route::link::nlas::Nla;
                match nlas {
                    Nla::IfName(name) => {
                        mydebug.debug_interfaces_detailed(format!("ifname: {}", name)).await;
                        ifn.ifname = name;
                    },
                    Nla::Mtu(bytes) => {
                        mydebug.debug_interfaces_detailed(format!("mtu: {}", bytes)).await;
                        ifn.mtu = bytes;
                    },
                    Nla::Address(addrset) => {
                        mydebug.debug_interfaces_detailed(
                            format!("lladdr: {:0x}:{:0x}:{:0x}:{:0x}:{:0x}:{:0x}",
                                    addrset[0], addrset[1],
                                    addrset[2], addrset[3],
                                    addrset[4], addrset[5])).await;
                    },
                    Nla::OperState(state) => {
                        if state == State::Up {
                            mydebug.debug_interfaces(format!("device {} is up", ifn.ifname)).await;
                        }
                        ifn.oper_state = state;
                    },
                    Nla::AfSpecInet(inets) => {
                        for ip in inets {
                            match ip {
                                AfSpecInet::Inet(_v4) => { },
                                AfSpecInet::Inet6(_v6) => {
                                    //mydebug.debug_verbose(format!("v6: {:?}", v6)).await;
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
            //mydebug.debug_interfaces(format!("")).await;
            (old_oper_state, ifn.oper_state, ifn.ifname.clone())
        };

        // now process result values from,
        // looking for interfaces which are now up, and which were not up before
        mydebug.debug_interfaces_detailed(format!("ifn: {:?} old: {:?} new: {:?}",
                                                  &ifname, old_oper_state,
                                                  new_oper_state)).await;
        if old_oper_state != State::Up && new_oper_state == State::Up {
            let     ifna = self.get_entry_by_ifindex(ifindex).await;
            let mut ifn  = ifna.lock().await;
            let mut used = 0;

            // looks like a new device that is now up!
            if options.is_valid_acp_interface(&ifname) {
                self.acp_interfaces.entry(ifindex).or_insert_with(|| {
                    ifna.clone()
                });
                mydebug.debug_info(format!("device {} now up as ACP", ifn.ifname)).await;
                ifn.start_acp(options, mydebug.clone(), self.invalidate_avail.clone()).await;
                used = 1;
            }

            if options.is_valid_joinlink_interface(&ifname) {
                let mut ji_hash = self.joinlink_interfaces.lock().await;
                ji_hash.entry(ifindex).or_insert_with(|| {
                    ifna.clone()
                });
                mydebug.debug_info(format!("device {} now up as Join Interface", ifn.ifname)).await;
                ifn.start_joinlink(allif.clone(), options,
                                   mydebug.clone(), self.invalidate_avail.clone()).await;
                used = used + 1;
            }

            if used == 0 {
                mydebug.debug_info(format!("interface {} ignored", ifn.ifname)).await;
            }
        }


        return ();
    }

    pub async fn gather_addr_info(lallif: &Arc<Mutex<AllInterfaces>>,
                                  options: &RoosterOptions,
                                  am: AddressMessage) -> Result<(), Error> {
        let mut allif   = lallif.lock().await;
        let _mydebug = allif.debug.clone();

        allif.store_addr_info(options,am).await;
        Ok(())
    }

    pub async fn gather_link_info(lallif: &Arc<Mutex<AllInterfaces>>,
                                  options: &RoosterOptions,
                                  debug:    Arc<DebugOptions>,
                                  lm: LinkMessage) -> Result<(), Error> {
        let mut allif   = lallif.lock().await;

        allif.store_link_info(lallif.clone(), options, debug, lm).await;
        Ok(())
    }

    pub async fn scan_existing_interfaces(lallif: &Arc<Mutex<AllInterfaces>>,
                                          options: &RoosterOptions,
                                          handle:  &Handle,
                                          debug:   Arc<DebugOptions>) -> Result<(), Error> {

        let mut list = handle.link().get().execute();
        let mut cnt: u32 = 0;
        let debugextra = Arc::new(DebugOptions {
            debug_interfaces: debug.debug_interfaces,
            debug_registrars: debug.debug_registrars,
            debug_joininterfaces: debug.debug_joininterfaces,
            debug_proxyactions: debug.debug_proxyactions,
            debug_output: debug.debug_output.clone()
        });

        debug.debug_info(format!("scanning existing interfaces")).await;

        while let Some(link) = list.try_next().await.unwrap() {
            debug.debug_info(format!("link message {}", cnt)).await;
            AllInterfaces::gather_link_info(&lallif,
                                            &options,
                                            debugextra.clone(),
                                            link).await.unwrap();
            cnt += 1;
        }

        let mut list = handle.address().get().execute();
        while let Some(addr) = list.try_next().await.unwrap() {
            debug.debug_info(format!("addr message {}", cnt)).await;
            AllInterfaces::gather_addr_info(&lallif,
                                            &options,
                                            addr).await.unwrap();
            cnt += 1;
        }
        Ok(())
    }

    pub async fn listen_network(lallif: &Arc<Mutex<AllInterfaces>>,
                                options: &RoosterOptions) ->
        Result<tokio::task::JoinHandle<Result<(),Error>>, String>
    {
        let myif = lallif.clone();
        let myoptions = options.clone();

        let listenhandle = tokio::spawn(async move {

            // Open the netlink socket
            let (mut connection, handle, mut messages) = new_connection().map_err(|e| format!("{}", e)).unwrap();

            let debug = {
                let allif   = myif.lock().await;
                allif.debug.clone()
            };

            // These flags specify what kinds of broadcast messages we want to listen for.
            let mgroup_flags = RTMGRP_IPV6_ROUTE | RTMGRP_IPV6_IFADDR | RTMGRP_LINK;

            // A netlink socket address is created with said flags.
            let addr = rtnetlink::sys::SocketAddr::new(0, mgroup_flags);

            // Said address is bound so new connections and
            // thus new message broadcasts can be received.
            connection.socket_mut().socket_mut().bind(&addr).expect("failed to bind");

            tokio::spawn(connection);

            debug.debug_info("scanning existing interfaces".to_string()).await;
            AllInterfaces::scan_existing_interfaces(&myif, &myoptions,
                                                    &handle, debug.clone()).await?;

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
                                                        &myoptions,
                                                        debug.clone(),
                                                        stuff).await.unwrap();
                    }
                    InnerMessage(NewAddress(stuff)) => {
                        let _sifn = AllInterfaces::gather_addr_info(&myif,
                                                                    &myoptions,
                                                                    stuff).await.unwrap();
                    }
                    InnerMessage(NewRoute(_thing)) => {
                        /* just ignore these! */
                    }
                    //_ => { println!("generic message type: {} skipped", payload.message_type()); }
                    _ => {
                        debug.debug_interfaces_detailed(format!("msg type: {:?}", payload)).await;
                    }
                }
            };
            Ok(())
        });
        Ok(listenhandle)
    }

    // goes through list of all registrars, and for each type of Registrar that we find
    // note that such a protocol is available.
    pub async fn calculate_available_registrars(self: &AllInterfaces) -> (bool, bool, bool) {
        let mut stateless_avail = false;
        let mut stateful_avail  = false;
        let mut http_avail      = false;
        for lai in self.acp_interfaces.values() {
            let ai = lai.lock().await;
            let (nhttp_avail, nstateful_avail, nstateless_avail) = ai.calculate_available_registrar().await;
            http_avail     = http_avail | nhttp_avail;
            stateful_avail = stateful_avail | nstateful_avail;
            stateless_avail= stateless_avail | nstateless_avail;
        }
        (http_avail, stateful_avail, stateless_avail)
    }

    pub async fn update_available_registrars(self: &mut AllInterfaces) {
        {
            let mut invalidated = self.invalidate_avail.lock().await;
            if *invalidated {
                let (http_avail, stateful_avail, stateless_avail) = self.calculate_available_registrars().await;
                self.proxies.http_avail=http_avail;
                self.proxies.stateful_avail=stateful_avail;
                self.proxies.stateless_avail=stateless_avail;
            }
            *invalidated = false;
        }
    }

    pub async fn pick_available_https_registrar(self: &AllInterfaces) -> Option<SocketAddr> {
        for lifn in self.acp_interfaces.values() {
            //println!("interfaces locking {:?}", chrono::offset::Local::now());
            let ifn = lifn.lock().await;
            if let Some(lai) = &ifn.acp_daemon {
                //println!("acp_interfaces locking {:?}", chrono::offset::Local::now());
                let ai = lai.lock().await;
                //println!("searching {:?} at {:?}", ai, chrono::offset::Local::now());
                for reg in &ai.registrars {
                    //println!("reg searching {:?}", reg);
                    for rtype in &reg.rtypes {
                        match rtype {
                            RegistrarType::HTTPRegistrar{tcp_port: port} => {
                                return Some(sockaddr_from_addr(reg.addr, *port))
                            },
                            _ => { },
                        }
                    }
                }
            }
        }
        return None;
    }

    pub async fn locked_pick_available_https_registrar(lallif: Arc<Mutex<AllInterfaces>>) -> Option<SocketAddr> {
        //println!("locked getting lock {:?}", chrono::offset::Local::now());
        let allif = lallif.lock().await;
        //println!("locked got lock {:?}", chrono::offset::Local::now());
        let reg = allif.pick_available_https_registrar().await;
        println!("picked registrar: {:?}", reg);
        return reg;
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
    extern crate hex_literal;
    use hex_literal::hex;

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
                                 debug_registrars:  false,
                                 debug_joininterfaces:  false,
                                 debug_proxyactions:    false,
                                 debug_output: awriter.clone() };
        let mut all1 = AllInterfaces::default();
        all1.debug = Arc::new(db1);

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

    /* define a second interface with ifindex and a Link-Local address, for Join */
    fn setup_am_2() -> AddressMessage {
        use netlink_packet_route::address::nlas::Nla;

        AddressMessage {
            header: AddressHeader { family: AF_INET6 as u8,
                                    prefix_len: 64,
                                    flags: 0,
                                    scope: 0,
                                    index: 12
            },
            nlas: vec![
                Nla::Address(vec![0xfe, 0x80, 0,0, 0,0,0,0,
                                  0x00, 0x00, 0,0, 0,0,0,2])
            ],
        }
    }


    async fn async_add_interface(allif: &mut AllInterfaces) -> Result<(), std::io::Error> {
        let options = RoosterOptions::default();
        assert_eq!(allif.interfaces.len(), 0);
        allif.store_addr_info(&options, setup_am()).await;
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
        let options = RoosterOptions::default();
        {
            let allif      = lallif.lock().await;
            assert_eq!(allif.interfaces.len(), 0);
        }
        AllInterfaces::gather_addr_info(lallif, &options, setup_am()).await.unwrap();
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
                Nla::Address(vec![0x52, 0x54, 0x00, 0x99, 0x9b, 0xba]),
                Nla::OperState(State::Up)
            ],
        }
    }

    /* define a new interface with ifindex and a Link-Local address */
    fn setup_lm_2() -> LinkMessage {
        use netlink_packet_route::link::nlas::Nla;

        LinkMessage {
            header: LinkHeader { interface_family: AF_INET6 as u8,
                                 index: 12,
                                 link_layer_type: ARPHRD_ETHER,
                                 flags: IFF_UP|IFF_LOWER_UP,
                                 change_mask: 0xffff_ffff
            },
            nlas: vec![
                Nla::IfName("join0".to_string()),
                Nla::Mtu(1500),
                Nla::Address(vec![0x52, 0x54, 0x00, 0x99, 0xa1, 0xab]),
                Nla::OperState(State::Up)
            ],
        }
    }

    async fn async_locked_add_link(lallif: &mut Arc<Mutex<AllInterfaces>>) -> Result<(), std::io::Error> {
        let options = RoosterOptions::default();
        let debug = {
            let allif      = lallif.lock().await;
            assert_eq!(allif.interfaces.len(), 0);
            allif.debug.clone()
        };
        AllInterfaces::gather_link_info(lallif, &options,
                                        debug.clone(),
                                        setup_lm()).await.unwrap();
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


    async fn async_enable_join_downstream(lallif:  Arc<Mutex<AllInterfaces>>,
                                          awriter: Arc<Mutex<Vec<u8>>>) -> Result<(), std::io::Error> {
        let mut options = RoosterOptions::default();
        let mut allif = lallif.lock().await;
        let debug = { allif.debug.clone() };
        options.debug_interfaces    = true;
        options.debug_registrars    = true;
        options.debug_joininterfaces  = true;

        options.add_acp_interface("eth0".to_string());
        options.add_joinlink_interface("join0".to_string());

        allif.store_link_info(lallif.clone(), &options, debug.clone(), setup_lm()).await;
        allif.store_addr_info(&options, setup_am()).await;
        allif.store_link_info(lallif.clone(), &options, debug.clone(), setup_lm_2()).await;
        allif.store_addr_info(&options, setup_am_2()).await;
        assert_eq!(allif.interfaces.len(), 2);

        allif.update_available_registrars().await;
        assert_eq!(allif.proxies.http_avail, false);
        {
            let invalidated = allif.invalidate_avail.lock().await;
            assert_eq!(*invalidated, false);
        }

        // now simulate receiving a GRASP message on this new interface.
        // first, go find interface
        let li10 = allif.get_entry_by_ifindex(10).await;
        // second, inject a message into that interface with announcement
        let m1   = crate::acp_interface::tests::msg1();
        {
            let i10 = li10.lock().await;

            /* i10 is now an *Interface*, look into it for a daemon */
            if let Some(lacp_daemon) = &i10.acp_daemon {
                let mut ad = lacp_daemon.lock().await;
                ad.registrar_announce(/*cnt*/1, m1).await;
            }
        }

        {
            let invalidated = allif.invalidate_avail.lock().await;
            assert_eq!(*invalidated, true);
        }

        allif.update_available_registrars().await;
        assert_eq!(allif.proxies.http_avail, true);

        // add interface two, set it as a join interface.
        let li12 = allif.get_entry_by_ifindex(12).await;
        let mflood = {
            let i12 = li12.lock().await;
            if let Some(jdaemon) = &i12.join_daemon {
                let mut jd = jdaemon.lock().await;
                jd.https_port = 1234;  // stuff the port number in unit test
                jd.build_an_proxy(&i12, allif.proxies.clone(), 1).await.unwrap()
            } else {
                vec![1u8]
            }
        };
        println!("{:02x?}", mflood);

        // /grasp-message/ [9, 1, /initiator/ h'00000000000000000000000000000000', 1,
        //   [/objective/ [/objective-name/ "AN_Proxy", 0, 1, /objective-value/ ""],
        //   [103, /ipv6-address/ h'00000000000000000000000000000000', 6, 38243]]]
        assert_eq!(mflood, hex!("
85                                      # array(5)
   09                                   # unsigned(9)
   01                                   # unsigned(1)
   50                                   # bytes(16)
      fe800000000000000000000000000002
   01                                   # unsigned(1)
   82                                   # array(2)
      84                                # array(4)
         68                             # text(8)
            414e5f50726f7879
         00                             # unsigned(0)
         01                             # unsigned(1)
         60                             # text(0)

      84                                # array(4)
         18 67                          # unsigned(103)
         50                             # bytes(16)
            fe800000000000000000000000000002
         06                             # unsigned(6)
         19 04d2                        # unsigned(1234)
"));

        let output = awriter.lock().await;
        let stuff = std::str::from_utf8(&output).unwrap();
        println!("{}",stuff);

        // now pick a registrar from the available ones, returning the SocketAddr for it.
        assert_eq!(allif.pick_available_https_registrar().await,
                   Some(std::net::SocketAddr::new(
                       "fda3:79a6:f6ee:0:200:0:6400:1".parse::<IpAddr>().unwrap(),
                       8993)));

        Ok(())
    }

    #[test]
    fn test_enable_join_downstream() -> Result<(), std::io::Error> {
        let (awriter, all1) = setup_ai();
        let lall = Arc::new(Mutex::new(all1));
        aw!(async_enable_join_downstream(lall, awriter)).unwrap();
        Ok(())
    }



}

/*
 * Local Variables:
 * mode: rust
 * compile-command: "cd .. && cargo test"
 * End:
 */

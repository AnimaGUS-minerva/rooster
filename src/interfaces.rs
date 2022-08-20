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

use tokio::runtime;
use std::net::Ipv6Addr;
use std::collections::HashMap;
use std::sync::Arc;
use futures::lock::Mutex;

use rtnetlink::{
    constants::{RTMGRP_IPV6_ROUTE, RTMGRP_IPV6_IFADDR, RTMGRP_LINK},
    Handle, Error,
    new_connection,
    sys::{AsyncSocket, SocketAddr},
};
use netlink_packet_route::{
    NetlinkPayload::InnerMessage,
    RtnlMessage::NewLink,
    RtnlMessage::NewAddress,
    RtnlMessage::NewRoute,
    RtnlMessage::DelRoute,
    LinkMessage, AddressMessage
};
use netlink_packet_route::link::nlas::AfSpecInet;
use netlink_packet_route::link::nlas::State;

use crate::debugoptions::DebugOptions;

pub type IfIndex = u32;

struct Interface {
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

struct AllInterfaces {
    pub debug:           DebugOptions,
    pub interfaces:      HashMap<u32, Arc<Mutex<Interface>>>,
    pub acp_interfaces:  HashMap<u32, Arc<Mutex<Interface>>>,
    pub downlink_interfaces:  HashMap<u32, Arc<Mutex<Interface>>>
}

impl AllInterfaces {
    pub fn default() -> Interface {
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
}


pub mod tests {
    use super::*;

    #[allow(unused_macros)]
    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
    }

    async fn async_search_entry(allif: &AllInterface) -> Result<(), std::io::Error> {
        let ifind01 = allif.get_entry_by_ifindex(1).await;
        assert_eq!(ifind01.ifindex, 1);
        ifind01.ifname = "eth0".to_string();

        let ifind02 = allif.get_entry_by_ifindex(2).await;
        assert_eq!(ifind02.ifindex, 2);
        ifind01.ifname = "eth0".to_string();   // yes, same name

        assert_eq!(allif.interfaces.size(), 2);
        OK(())
    }

    #[test]
    fn test_search_entry() -> Result<(), std::io::Error> {
        let writer: Vec<u8> = vec![];
        let awriter = Arc::new(Mutex::new(writer));
        let db1 = DebugOptions { debug_interfaces: true,
                                 debug_output: awriter.clone() };
        let all1 = AllInterface::default().debug = db1;

        aw!(async_search_entry(&all1)).unwrap();
        Ok(())
    }
}

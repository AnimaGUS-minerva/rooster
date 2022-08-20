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
}


pub mod tests {
    use super::*;

    #[allow(unused_macros)]
    macro_rules! aw {
        ($e:expr) => {
            tokio_test::block_on($e)
        };
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

        assert_eq!(allif.interfaces.len(), 2);
        Ok(())
    }

    #[test]
    fn test_search_entry() -> Result<(), std::io::Error> {
        let writer: Vec<u8> = vec![];
        let awriter = Arc::new(Mutex::new(writer));
        let db1 = DebugOptions { debug_interfaces: true,
                                 debug_output: awriter.clone() };
        let mut all1 = AllInterfaces::default();
        all1.debug = db1;

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

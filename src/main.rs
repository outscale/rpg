/* Copyright 2017 Outscale SAS
 *
 * This file is part of RPG - Remote PacketGraph.
 *
 * Pg is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License version 3 as published
 * by the Free Software Foundation.
 *
 * Packetgraph is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with Packetgraph.  If not, see <http://www.gnu.org/licenses/>.
 */

#![feature(proc_macro_hygiene, decl_macro)]

#[macro_use] extern crate rocket;
#[macro_use] extern crate rocket_contrib;
#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate pg;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use pg::{Brick, Graph, Nop, Firewall, Switch, Tap, Hub, Side, Nic};
use rocket::{State, Rocket};
use rocket::request::Form;
use rocket_contrib::json::{Json, JsonValue};
use rocket::response::content::Content;
use rocket::http::ContentType;
use std::thread;
use std::str::FromStr;

static API_VERSION: &'static str = "0.1.0";

struct RpgGraph {
    graph: Graph,
    run: bool,
}

type GraphMap = Arc<RwLock<HashMap<String, Arc<RwLock<RpgGraph>>>>>;

#[derive(Serialize)]
struct BrickDescription {
    name: String,
    type_name: String,
}

impl BrickDescription {
    fn new(brick: &Brick) -> BrickDescription {
        BrickDescription {
            name: brick.name(),
            type_name: String::from(brick.type_str()),
        }
    }
}

#[derive(Serialize)]
struct GraphDescription {
    name: String,
    bricks: Vec<String>,
}

impl GraphDescription {
    fn new(graph: &Graph) -> GraphDescription {
        let mut bricks = Vec::new();
        for name in graph.bricks.keys() {
            bricks.push(name.clone());
        }
        GraphDescription {
            name: graph.name.clone(),
            bricks: bricks,
        }
    }
}

fn result<S: Into<String>>(status: bool, description: S) -> Json<JsonValue> {
    let d = description.into();
    Json(json!({
        "status": match status {
            true => "ok",
            false => "error",
        },
        "description": d.as_str(),
    }))
}

#[derive(Serialize)]
struct ApiDescription {
    version: String
}

#[get("/")]
fn index() -> Json<ApiDescription> {
    Json(ApiDescription{version: String::from(API_VERSION)})
}

#[get("/graph")]
fn graphs(graphs: State<GraphMap>) -> Option<Json<Vec<String>>> {
    let map = graphs.read().unwrap();
    let mut res = Vec::<String>::new();
    for (name, _) in map.iter() {
        res.push(name.clone());
    }
    return Some(Json(res));
}

#[derive(FromForm)]
struct GraphCreation {
    name: String
}

#[get("/graph/new?<graph..>")]
fn graph_new(graphs: State<GraphMap>, graph: Form<GraphCreation>) -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    if map.get(&graph.name).is_some() {
        return Some(result(false, "graph already exists"));
    }
    let new_graph = Arc::new(RwLock::new(RpgGraph {
                                             graph: Graph::new(graph.name.clone()),
                                             run: true,
                                         }));
    let ng = new_graph.clone();
    thread::spawn(move || pooler(ng));
    map.insert(graph.name.clone(), new_graph);
    return Some(result(true, ""));
}

#[get("/graph/<graph_name>")]
fn graph_get(graphs: State<GraphMap>, graph_name: String) -> Option<Json<GraphDescription>> {
    let map = graphs.read().unwrap();
    let g = match map.get(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let g = g.read().unwrap();
    let desc = GraphDescription::new(&g.graph);
    return Some(Json(desc));
}

#[get("/graph/<graph_name>/delete")]
fn graph_delete(graphs: State<GraphMap>, graph_name: String) -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    match map.remove(&graph_name) {
        Some(g) => {
            let mut g = g.write().unwrap();
            g.run = false;
            Some(result(true, ""))
        }
        None => None,
    }
}

#[get("/graph/<graph_name>/dot")]
fn dot_get(graphs: State<GraphMap>, graph_name: String) -> Option<String> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };
    let mut g = g.write().unwrap();
    match g.graph.dot() {
        Err(_) => Some(String::new()),
        Ok(s) => Some(s),
    }
}

#[get("/graph/<graph_name>/svg")]
fn dot_get_svg(graphs: State<GraphMap>, graph_name: String) -> Option<Content<String>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };
    let mut g = g.write().unwrap();
    match g.graph.svg() {
        Err(_) => None,
        Ok(s) => Some(Content(ContentType::SVG, s)),
    }
}

#[get("/graph/<graph_name>/brick/<brick_name>")]
fn brick_get(graphs: State<GraphMap>,
             graph_name: String,
             brick_name: String)
             -> Option<Json<BrickDescription>> {
    let map = graphs.read().unwrap();
    let g = match map.get(&graph_name) {
        Some(g) => g,
        None => return None,
    };
    let g = g.write().unwrap();
    let b = match g.graph.bricks.get(&brick_name) {
        Some(b) => b,
        None => return None,
    };
    let desc = BrickDescription::new(b);
    return Some(Json(desc));
}

#[derive(FromForm)]
struct LinkCreation {
    west: String,
    east: String,
}

#[get("/graph/<graph_name>/brick/link?<link..>")]
fn link(graphs: State<GraphMap>, graph_name: String, link: Form<LinkCreation>) -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    let west = g.graph.bricks.remove(&link.west);
    let east = g.graph.bricks.remove(&link.east);
    let mut ret: Option<Json<JsonValue>> = None;

    if west.is_some() && east.is_some() {
        let mut w = west.unwrap();
        let mut e = east.unwrap();
        ret = match w.link(&mut e) {
            Ok(()) => Some(result(true, "")),
            Err(e) => Some(result(false, format!("{}", e))),
        };
        g.graph.bricks.insert(link.west.clone(), w);
        g.graph.bricks.insert(link.east.clone(), e);
    } else if west.is_none() && east.is_none() {
        ret = Some(result(false, "west and east bricks not found"));
    } else if west.is_none() {
        g.graph.bricks.insert(link.east.clone(), east.unwrap());
        ret = Some(result(false, "west brick not found"));
    } else if east.is_none() {
        g.graph.bricks.insert(link.west.clone(), west.unwrap());
        ret = Some(result(false, "east brick not found"));
    }
    return ret;
}

#[derive(FromForm)]
struct LinkDeletion {
    west: String,
    east: String,
}

#[get("/graph/<graph_name>/brick/unlink?<unlink..>")]
fn unlink_from(graphs: State<GraphMap>, graph_name: String, unlink: Form<LinkDeletion>) -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    let west = g.graph.bricks.remove(&unlink.west);
    let east = g.graph.bricks.remove(&unlink.east);
    let mut ret: Option<Json<JsonValue>> = None;

    if west.is_some() && east.is_some() {
        let mut w = west.unwrap();
        let mut e = east.unwrap();
        ret = match w.unlink_from(&mut e) {
            Ok(()) => Some(result(true, "")),
            Err(e) => Some(result(false, format!("{}", e))),
        };
        g.graph.bricks.insert(unlink.west.clone(), w);
        g.graph.bricks.insert(unlink.east.clone(), e);
    } else if west.is_none() && east.is_none() {
        ret = Some(result(false, "west and east bricks not found"));
    } else if west.is_none() {
        g.graph.bricks.insert(unlink.east.clone(), east.unwrap());
        ret = Some(result(false, "west brick not found"));
    } else if east.is_none() {
        g.graph.bricks.insert(unlink.west.clone(), west.unwrap());
        ret = Some(result(false, "east brick not found"));
    }
    return ret;
}

#[get("/graph/<graph_name>/brick/<brick_name>/unlink")]
fn unlink(graphs: State<GraphMap>,
        graph_name: String,
        brick_name: String)
    -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    let b = match g.graph.bricks.get_mut(&brick_name) {
        Some(b) => b,
        None => return None,
    };
    b.unlink();
    Some(result(true, ""))
}



#[get("/graph/<graph_name>/brick/<brick_name>/delete")]
fn brick_delete(graphs: State<GraphMap>,
                graph_name: String,
                brick_name: String)
                -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    match g.graph.bricks.remove(&brick_name) {
        None => None,
        Some(_) => Some(result(true, "")),
    }
}

#[derive(FromForm)]
struct NopCreation {
    name: String,
}

#[get("/graph/<graph_name>/brick/new/nop?<nop..>")]
fn nop_new(graphs: State<GraphMap>, graph_name: String, nop: Form<NopCreation>) -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    if g.graph.bricks.get(&nop.name).is_some() {
        return Some(result(false, "brick already exists"));
    }

    g.graph
        .bricks
        .insert(nop.name.clone(), Brick::Nop(Nop::new(nop.name.clone())));
    Some(result(true, ""))
}

#[derive(FromForm)]
struct TapCreation {
    name: String,
}

#[get("/graph/<graph_name>/brick/new/tap?<tap..>")]
fn tap_new(graphs: State<GraphMap>, graph_name: String, tap: Form<TapCreation>) -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    if g.graph.bricks.get(&tap.name).is_some() {
        return Some(result(false, "brick already exists"));
    }

    g.graph
        .bricks
        .insert(tap.name.clone(), Brick::Tap(Tap::new(tap.name.clone())));
    Some(result(true, ""))
}

#[derive(FromForm)]
struct HubCreation {
    name: String,
    west_ports: u32,
    east_ports: u32,
}

#[get("/graph/<graph_name>/brick/new/hub?<hub..>")]
fn hub_new(graphs: State<GraphMap>, graph_name: String, hub: Form<HubCreation>) -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    if g.graph.bricks.get(&hub.name).is_some() {
        return Some(result(false, "brick already exists"));
    }

    g.graph
        .bricks
        .insert(hub.name.clone(),
                Brick::Hub(Hub::new(hub.name.clone(), hub.west_ports, hub.east_ports)));
    Some(result(true, ""))
}

#[derive(FromForm)]
struct SwitchCreation {
    name: String,
    west_ports: u32,
    east_ports: u32,
    side: String,
}

#[get("/graph/<graph_name>/brick/new/switch?<switch..>")]
fn switch_new(graphs: State<GraphMap>,
              graph_name: String,
              switch: Form<SwitchCreation>)
              -> Option<Json<JsonValue>> {
    let side = match Side::from_str(switch.side.as_str()) {
        Ok(s) => s,
        Err(_) => return Some(result(false, "choose west or east for side parameter")),
    };
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    if g.graph.bricks.get(&switch.name).is_some() {
        return Some(result(false, "brick already exists"));
    }

    g.graph
        .bricks
        .insert(switch.name.clone(),
                Brick::Switch(Switch::new(switch.name.clone(),
                                          switch.west_ports,
                                          switch.east_ports,
                                          side)));
    Some(result(true, ""))
}

#[derive(FromForm)]
struct NicCreation {
    name: String,
    vdev: Option<String>,
    port: Option<u8>,
}

#[get("/graph/<graph_name>/brick/new/nic?<nic..>")]
fn nic_new(graphs: State<GraphMap>, graph_name: String, nic: Form<NicCreation>) -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    if nic.vdev.is_none() && nic.port.is_none() {
        return Some(result(false, "must specify either 'port' or 'vdev' parameters"));
    }

    let mut g = g.write().unwrap();
    if g.graph.bricks.get(&nic.name).is_some() {
        return Some(result(false, "brick already exists"));
    }

    let nic_brick = match nic.vdev.is_some() {
        true => Nic::new(nic.name.clone(), nic.vdev.clone().unwrap()),
        false => Nic::new_port(nic.name.clone(), nic.port.unwrap()),
    };
    let nic_brick = match nic_brick {
        Ok(n) => n,
        Err(e) => return Some(result(false, format!("cannot create nic: {}", e))),
    };
    g.graph.bricks.insert(nic.name.clone(), Brick::Nic(nic_brick));
    Some(result(true, ""))
}

#[derive(FromForm)]
struct FirewallCreation {
    name: String,
}

#[get("/graph/<graph_name>/brick/new/firewall?<firewall..>")]
fn firewall_new(graphs: State<GraphMap>,
                graph_name: String,
                firewall: Form<FirewallCreation>)
                -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    if g.graph.bricks.get(&firewall.name).is_some() {
        return Some(result(false, "brick already exists"));
    }

    g.graph
        .bricks
        .insert(firewall.name.clone(),
                Brick::Firewall(Firewall::new(firewall.name.clone())));
    Some(result(true, ""))
}

#[derive(FromForm)]
struct FirewallRule {
    filter: String,
    side: String,
}

#[get("/graph/<graph_name>/brick/<brick_name>/firewall/rule?<rule..>")]
fn firewall_rule_add(graphs: State<GraphMap>,
                     graph_name: String,
                     brick_name: String,
                     rule: Form<FirewallRule>)
                     -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    let b = match g.graph.bricks.get_mut(&brick_name) {
        Some(b) => b,
        None => return None,
    };

    let fw = match b.firewall() {
        Some(fw) => fw,
        None => return None,
    };

    let side = match Side::from_str(rule.side.as_str()) {
        Ok(s) => s,
        Err(_) => return Some(result(false, "choose west or east for side parameter")),
    };

    match fw.rule_add(rule.filter.clone(), side) {
        Ok(_) => Some(result(true, "")),
        Err(e) => Some(result(false, format!("{}", e))),
    }
}

#[get("/graph/<graph_name>/brick/<brick_name>/firewall/flush")]
fn firewall_flush(graphs: State<GraphMap>,
                  graph_name: String,
                  brick_name: String)
                  -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    let b = match g.graph.bricks.get_mut(&brick_name) {
        Some(b) => b,
        None => return None,
    };

    let fw = match b.firewall() {
        Some(fw) => fw,
        None => return None,
    };

    fw.flush();
    Some(result(true, ""))
}

#[get("/graph/<graph_name>/brick/<brick_name>/firewall/reload")]
fn firewall_reload(graphs: State<GraphMap>,
                   graph_name: String,
                   brick_name: String)
                   -> Option<Json<JsonValue>> {
    let mut map = graphs.write().unwrap();
    let g = match map.get_mut(&graph_name) {
        Some(g) => g,
        None => return None,
    };

    let mut g = g.write().unwrap();
    let b = match g.graph.bricks.get_mut(&brick_name) {
        Some(b) => b,
        None => return None,
    };

    let fw = match b.firewall() {
        Some(fw) => fw,
        None => return None,
    };

    match fw.reload() {
        Ok(_) => Some(result(true, "")),
        Err(e) => Some(result(false, format!("{}", e))),
    }
}

fn pooler(graph: Arc<RwLock<RpgGraph>>) {
    loop {
        let mut g = graph.write().unwrap();
        match g.run {
            true => {
                g.graph.poll();
            }
            false => break,
        }
    }
}

fn rocket_init() -> Rocket {
    pg::init();
    let graphs = Arc::new(RwLock::new(HashMap::<String, Arc<RwLock<RpgGraph>>>::new()));
    rocket::ignite()
        .manage(graphs)
        .mount("/", routes![index,
                            graphs,
                            graph_new,
                            graph_get,
                            graph_delete,
                            brick_get,
                            link,
                            unlink,
                            unlink_from,
                            dot_get,
                            dot_get_svg,
                            brick_delete,
                            nop_new,
                            tap_new,
                            hub_new,
                            switch_new,
                            nic_new,
                            firewall_new,
                            firewall_rule_add,
                            firewall_flush,
                            firewall_reload])
}

fn main() {
    rocket_init().launch();
}

#[cfg(test)]
mod test {
    use super::*;
    use rocket::local::Client;
    use rocket::http::Status;

    fn request_ok(client: &Client, url: &'static str) {
        let response = client.get(url).dispatch();
        assert_eq!(response.status(), Status::Ok);
        // TODO: check some response content
        //let body_str = response.body().and_then(|b| b.into_string());
        //assert_eq!(body_str, Some("Hello, world!".to_string()));
    }

    #[test]
    fn simple() {
        let r = rocket_init();
        let c = Client::new(r).expect("valid rocket instance");
        request_ok(&c, "/graph/new?name=mygraph");
        request_ok(&c, "/graph/mygraph");
        request_ok(&c, "/graph/mygraph/brick/new/tap?name=tap1");
        request_ok(&c, "/graph/mygraph/brick/new/nop?name=nop1");
        request_ok(&c, "/graph/mygraph/brick/new/tap?name=tap2");
        request_ok(&c, "/graph/mygraph/brick/new/switch?name=switch1&west_ports=2&east_ports=2&side=west");
        request_ok(&c, "/graph/mygraph/brick/tap1");
        request_ok(&c, "/graph/mygraph/brick/nop1");
        request_ok(&c, "/graph/mygraph/brick/tap2");
        request_ok(&c, "/graph/mygraph/brick/switch1");
        request_ok(&c, "/graph/mygraph/brick/link?west=tap1&east=switch1");
        request_ok(&c, "/graph/mygraph/brick/link?west=nop1&east=switch1");
        request_ok(&c, "/graph/mygraph/brick/link?west=switch1&east=tap2");
        request_ok(&c, "/graph/mygraph/brick/unlink?west=tap1&east=switch1");
        request_ok(&c, "/graph/mygraph");
        request_ok(&c, "/graph/mygraph/brick/switch1/unlink");
        request_ok(&c, "/graph/mygraph/delete");
    }

    #[test]
    fn firewall() {
        let r = rocket_init();
        let c = Client::new(r).expect("valid rocket instance");
        request_ok(&c, "/graph/new?name=mygraph");
        request_ok(&c, "/graph/mygraph");
        request_ok(&c, "/graph/mygraph/brick/new/tap?name=tap1");
        request_ok(&c, "/graph/mygraph/brick/new/firewall?name=fw");
        request_ok(&c, "/graph/mygraph/brick/new/tap?name=tap2");
        request_ok(&c, "/graph/mygraph/brick/tap1");
        request_ok(&c, "/graph/mygraph/brick/fw");
        request_ok(&c, "/graph/mygraph/brick/tap2");
        request_ok(&c, "/graph/mygraph/brick/link?west=tap1&east=fw");
        request_ok(&c, "/graph/mygraph/brick/link?west=fw&east=tap2");
        request_ok(&c, "/graph/mygraph/brick/fw/firewall/rule?side=west&filter=src%20host%2010%3A%3A1");
        request_ok(&c, "/graph/mygraph/brick/fw/firewall/flush");
        request_ok(&c, "/graph/mygraph/brick/fw/firewall/rule?side=west&filter=src%20host%2010%3A%3A1");
        request_ok(&c, "/graph/mygraph/brick/fw/firewall/rule?side=west&filter=src%20host%2010%3A%3A2");
        request_ok(&c, "/graph/mygraph/brick/fw/firewall/reload");
    }
}

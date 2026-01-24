//! Lua bindings for Mesh and MeshBuilder
//!
//! Provides Lua wrappers for network topology creation and inspection.

use mlua::{FromLua, Lua, MetaMethod, Result, Table, UserData, UserDataMethods, Value};
use std::sync::{Arc, RwLock};

use crate::topology::{Mesh, MeshBuilder, from_edges};

use super::types::LuaPeerId;

/// Lua wrapper for Mesh (thread-safe with interior mutability)
#[derive(Clone)]
pub struct LuaMesh(pub Arc<RwLock<Mesh>>);

impl LuaMesh {
    pub fn new(mesh: Mesh) -> Self {
        Self(Arc::new(RwLock::new(mesh)))
    }

    pub fn inner(&self) -> Arc<RwLock<Mesh>> {
        self.0.clone()
    }
}

impl From<Mesh> for LuaMesh {
    fn from(mesh: Mesh) -> Self {
        Self::new(mesh)
    }
}

impl FromLua for LuaMesh {
    fn from_lua(value: Value, _lua: &Lua) -> Result<Self> {
        match value {
            Value::UserData(ud) => ud.borrow::<Self>().map(|v| v.clone()),
            _ => Err(mlua::Error::external("Expected Mesh userdata")),
        }
    }
}

impl UserData for LuaMesh {
    fn add_fields<F: mlua::UserDataFields<Self>>(_fields: &mut F) {
        // Read-only field accessors via methods would be cleaner,
        // but fields give nicer Lua syntax (mesh.peer_count vs mesh:peer_count())
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // peer_count() -> number
        methods.add_method("peer_count", |_, this, ()| {
            let mesh = this
                .0
                .read()
                .map_err(|_| mlua::Error::external("Mesh lock poisoned"))?;
            Ok(mesh.peer_count())
        });

        // edge_count() -> number
        methods.add_method("edge_count", |_, this, ()| {
            let mesh = this
                .0
                .read()
                .map_err(|_| mlua::Error::external("Mesh lock poisoned"))?;
            Ok(mesh.edge_count())
        });

        // peers() -> [PeerId]
        methods.add_method("peers", |_, this, ()| {
            let mesh = this
                .0
                .read()
                .map_err(|_| mlua::Error::external("Mesh lock poisoned"))?;
            let peers: Vec<LuaPeerId> = mesh.peer_ids().into_iter().map(LuaPeerId).collect();
            Ok(peers)
        });

        // neighbors(peer) -> [PeerId]
        methods.add_method("neighbors", |_, this, peer: LuaPeerId| {
            let mesh = this
                .0
                .read()
                .map_err(|_| mlua::Error::external("Mesh lock poisoned"))?;
            let neighbors: Vec<LuaPeerId> = mesh
                .neighbors(peer.0)
                .map(|n| n.iter().copied().map(LuaPeerId).collect())
                .unwrap_or_default();
            Ok(neighbors)
        });

        // are_connected(a, b) -> bool
        methods.add_method(
            "are_connected",
            |_, this, (a, b): (LuaPeerId, LuaPeerId)| {
                let mesh = this
                    .0
                    .read()
                    .map_err(|_| mlua::Error::external("Mesh lock poisoned"))?;
                Ok(mesh.are_connected(a.0, b.0))
            },
        );

        // mutual_peers(a, b) -> [PeerId]
        methods.add_method("mutual_peers", |_, this, (a, b): (LuaPeerId, LuaPeerId)| {
            let mesh = this
                .0
                .read()
                .map_err(|_| mlua::Error::external("Mesh lock poisoned"))?;
            let mutual: Vec<LuaPeerId> = mesh
                .mutual_peers(a.0, b.0)
                .into_iter()
                .map(LuaPeerId)
                .collect();
            Ok(mutual)
        });

        // visualize() -> string
        methods.add_method("visualize", |_, this, ()| {
            let mesh = this
                .0
                .read()
                .map_err(|_| mlua::Error::external("Mesh lock poisoned"))?;
            Ok(mesh.visualize())
        });

        // connect(a, b) - add an edge
        methods.add_method("connect", |_, this, (a, b): (LuaPeerId, LuaPeerId)| {
            let mut mesh = this
                .0
                .write()
                .map_err(|_| mlua::Error::external("Mesh lock poisoned"))?;
            mesh.connect(a.0, b.0);
            Ok(())
        });

        // String representation
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            let mesh = this
                .0
                .read()
                .map_err(|_| mlua::Error::external("Mesh lock poisoned"))?;
            Ok(format!(
                "Mesh({} peers, {} edges)",
                mesh.peer_count(),
                mesh.edge_count()
            ))
        });
    }
}

/// Lua wrapper for MeshBuilder
pub struct LuaMeshBuilder {
    peer_count: usize,
}

impl UserData for LuaMeshBuilder {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // ring() -> Mesh
        methods.add_method("ring", |_, this, ()| {
            let mesh = MeshBuilder::new(this.peer_count).ring();
            Ok(LuaMesh::new(mesh))
        });

        // full_mesh() -> Mesh
        methods.add_method("full_mesh", |_, this, ()| {
            let mesh = MeshBuilder::new(this.peer_count).full_mesh();
            Ok(LuaMesh::new(mesh))
        });

        // line() -> Mesh
        methods.add_method("line", |_, this, ()| {
            let mesh = MeshBuilder::new(this.peer_count).line();
            Ok(LuaMesh::new(mesh))
        });

        // star() -> Mesh
        methods.add_method("star", |_, this, ()| {
            let mesh = MeshBuilder::new(this.peer_count).star();
            Ok(LuaMesh::new(mesh))
        });

        // random(probability) -> Mesh
        methods.add_method("random", |_, this, prob: f64| {
            if !(0.0..=1.0).contains(&prob) {
                return Err(mlua::Error::external(
                    "Probability must be between 0.0 and 1.0",
                ));
            }
            let mesh = MeshBuilder::new(this.peer_count).random(prob);
            Ok(LuaMesh::new(mesh))
        });

        // String representation
        methods.add_meta_method(MetaMethod::ToString, |_, this, ()| {
            Ok(format!("MeshBuilder({} peers)", this.peer_count))
        });
    }
}

/// Register mesh constructors with the indras table
pub fn register(lua: &Lua, indras: &Table) -> Result<()> {
    // MeshBuilder constructor table
    let mesh_builder = lua.create_table()?;

    // MeshBuilder.new(n) -> MeshBuilder
    mesh_builder.set(
        "new",
        lua.create_function(|_, n: usize| {
            if n == 0 || n > 26 {
                return Err(mlua::Error::external("Peer count must be 1-26"));
            }
            Ok(LuaMeshBuilder { peer_count: n })
        })?,
    )?;

    indras.set("MeshBuilder", mesh_builder)?;

    // Mesh constructor table
    let mesh = lua.create_table()?;

    // Mesh.new() -> empty Mesh
    mesh.set(
        "new",
        lua.create_function(|_, ()| Ok(LuaMesh::new(Mesh::new())))?,
    )?;

    // Mesh.from_edges({{A,B}, {B,C}}) -> Mesh
    mesh.set(
        "from_edges",
        lua.create_function(|_, edges: Table| {
            let mut edge_list = Vec::new();

            for pair in edges.sequence_values::<Table>() {
                let pair = pair?;
                let a: Value = pair.get(1)?;
                let b: Value = pair.get(2)?;

                let a_char = match a {
                    Value::String(s) => s.to_str()?.chars().next().ok_or_else(|| {
                        mlua::Error::external("Edge requires single character strings")
                    })?,
                    Value::UserData(ud) => {
                        let peer: LuaPeerId = ud.take()?;
                        peer.0.0
                    }
                    _ => return Err(mlua::Error::external("Edge must be string or PeerId")),
                };

                let b_char = match b {
                    Value::String(s) => s.to_str()?.chars().next().ok_or_else(|| {
                        mlua::Error::external("Edge requires single character strings")
                    })?,
                    Value::UserData(ud) => {
                        let peer: LuaPeerId = ud.take()?;
                        peer.0.0
                    }
                    _ => return Err(mlua::Error::external("Edge must be string or PeerId")),
                };

                edge_list.push((a_char, b_char));
            }

            let mesh = from_edges(&edge_list);
            Ok(LuaMesh::new(mesh))
        })?,
    )?;

    indras.set("Mesh", mesh)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let indras = lua.create_table().unwrap();
        super::super::types::register(&lua, &indras).unwrap();
        register(&lua, &indras).unwrap();
        lua.globals().set("indras", indras).unwrap();
        lua
    }

    #[test]
    fn test_mesh_builder_full() {
        let lua = setup_lua();

        let (peers, edges): (usize, usize) = lua
            .load(
                r#"
                local mesh = indras.MeshBuilder.new(3):full_mesh()
                return mesh:peer_count(), mesh:edge_count()
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(peers, 3);
        assert_eq!(edges, 3); // C(3,2) = 3
    }

    #[test]
    fn test_mesh_builder_ring() {
        let lua = setup_lua();

        let edges: usize = lua
            .load(
                r#"
                local mesh = indras.MeshBuilder.new(4):ring()
                return mesh:edge_count()
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(edges, 4); // 4 peers in a ring = 4 edges
    }

    #[test]
    fn test_mesh_from_edges() {
        let lua = setup_lua();

        let result: bool = lua
            .load(
                r#"
                local mesh = indras.Mesh.from_edges({{'A','B'}, {'B','C'}})
                return mesh:peer_count() == 3 and mesh:edge_count() == 2
            "#,
            )
            .eval()
            .unwrap();
        assert!(result);
    }

    #[test]
    fn test_mesh_neighbors() {
        let lua = setup_lua();

        let count: i32 = lua
            .load(
                r#"
                local mesh = indras.MeshBuilder.new(3):full_mesh()
                local neighbors = mesh:neighbors(indras.PeerId.new('A'))
                return #neighbors
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(count, 2); // A is connected to B and C
    }

    #[test]
    fn test_mesh_are_connected() {
        let lua = setup_lua();

        let (ab, ac): (bool, bool) = lua
            .load(
                r#"
                local mesh = indras.Mesh.from_edges({{'A','B'}})
                local a = indras.PeerId.new('A')
                local b = indras.PeerId.new('B')
                local c = indras.PeerId.new('C')
                return mesh:are_connected(a, b), mesh:are_connected(a, c)
            "#,
            )
            .eval()
            .unwrap();
        assert!(ab);
        assert!(!ac);
    }

    #[test]
    fn test_mesh_mutual_peers() {
        let lua = setup_lua();

        let count: i32 = lua
            .load(
                r#"
                -- A-B-C line, so A and C share B as mutual
                local mesh = indras.Mesh.from_edges({{'A','B'}, {'B','C'}})
                local a = indras.PeerId.new('A')
                local c = indras.PeerId.new('C')
                local mutual = mesh:mutual_peers(a, c)
                return #mutual
            "#,
            )
            .eval()
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_mesh_visualize() {
        let lua = setup_lua();

        let viz: String = lua
            .load(
                r#"
                local mesh = indras.MeshBuilder.new(2):full_mesh()
                return mesh:visualize()
            "#,
            )
            .eval()
            .unwrap();
        assert!(viz.contains("Peers: 2"));
    }
}

//! Lua bindings for App
//!
//! Provides UserData implementation for the main App type.

use mlua::{Lua, Result, Table, UserData, UserDataMethods};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::app::App;
use crate::storage::LocalStorage;

use super::note::LuaNote;
use super::notebook::LuaNotebook;

/// Lua wrapper for App with async method wrappers
#[derive(Clone)]
pub struct LuaApp {
    inner: Arc<Mutex<App>>,
}

impl LuaApp {
    pub fn new(app: Arc<Mutex<App>>) -> Self {
        Self { inner: app }
    }

    pub fn inner(&self) -> Arc<Mutex<App>> {
        Arc::clone(&self.inner)
    }
}

impl UserData for LuaApp {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // init(name) - Initialize the app with a new identity
        methods.add_method("init", |_, this, name: String| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let mut app = app.lock().await;
                app.init(&name).await
            });
            result.map_err(|e| mlua::Error::external(e.to_string()))
        });

        // load() - Load existing profile
        methods.add_method("load", |_, this, ()| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let mut app = app.lock().await;
                app.load().await
            });
            result.map_err(|e| mlua::Error::external(e.to_string()))
        });

        // is_initialized() -> bool
        methods.add_method("is_initialized", |_, this, ()| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let app = app.lock().await;
                app.is_initialized().await
            });
            Ok(result)
        });

        // user_name() -> string or nil
        methods.add_method("user_name", |_, this, ()| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let app = app.lock().await;
                app.user_name().map(|s| s.to_string())
            });
            Ok(result)
        });

        // user_short_id() -> string or nil (peer_id alias)
        methods.add_method("user_short_id", |_, this, ()| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let app = app.lock().await;
                app.user_short_id()
            });
            Ok(result)
        });

        // peer_id() -> string or nil
        methods.add_method("peer_id", |_, this, ()| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let app = app.lock().await;
                app.user_short_id()
            });
            Ok(result)
        });

        // create_notebook(name) -> interface_id_hex
        methods.add_method("create_notebook", |_, this, name: String| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let mut app = app.lock().await;
                app.create_notebook(&name).await
            });
            result
                .map(|id| hex::encode(id.as_bytes()))
                .map_err(|e| mlua::Error::external(e.to_string()))
        });

        // list_notebooks() -> table of {name, interface_id, note_count}
        methods.add_method("list_notebooks", |lua, this, ()| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let app = app.lock().await;
                app.list_notebooks().await
            });

            let notebooks = result.map_err(|e| mlua::Error::external(e.to_string()))?;

            let table = lua.create_table()?;
            for (i, nb) in notebooks.iter().enumerate() {
                let entry = lua.create_table()?;
                entry.set("name", nb.name.clone())?;
                entry.set("interface_id", hex::encode(nb.interface_id.as_bytes()))?;
                entry.set("note_count", nb.note_count)?;
                table.set(i + 1, entry)?;
            }
            Ok(table)
        });

        // open_notebook(interface_id_hex)
        methods.add_method("open_notebook", |_, this, interface_id_hex: String| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let interface_id_bytes = hex::decode(&interface_id_hex)
                    .map_err(|e| crate::app::AppError::NotebookNotFound(e.to_string()))?;
                let interface_id = indras_core::InterfaceId::from_slice(&interface_id_bytes)
                    .ok_or_else(|| crate::app::AppError::NotebookNotFound("Invalid interface ID length".to_string()))?;

                let mut app = app.lock().await;
                app.open_notebook(&interface_id).await
            });
            result.map_err(|e| mlua::Error::external(e.to_string()))
        });

        // close_notebook()
        methods.add_method("close_notebook", |_, this, ()| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let mut app = app.lock().await;
                app.close_notebook().await
            });
            result.map_err(|e| mlua::Error::external(e.to_string()))
        });

        // current_notebook() -> LuaNotebook or nil
        methods.add_method("current_notebook", |_, this, ()| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let app = app.lock().await;
                app.current_notebook().cloned()
            });
            Ok(result.map(LuaNotebook::new))
        });

        // create_note(title) -> note_id
        methods.add_method("create_note", |_, this, title: String| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let mut app = app.lock().await;
                app.create_note(&title).await
            });
            result.map_err(|e| mlua::Error::external(e.to_string()))
        });

        // update_note_content(note_id, content)
        methods.add_method(
            "update_note_content",
            |_, this, (note_id, content): (String, String)| {
                let app = Arc::clone(&this.inner);
                let result = futures::executor::block_on(async {
                    let mut app = app.lock().await;
                    app.update_note_content(&note_id, &content).await
                });
                result.map_err(|e| mlua::Error::external(e.to_string()))
            },
        );

        // update_note_title(note_id, title)
        methods.add_method(
            "update_note_title",
            |_, this, (note_id, title): (String, String)| {
                let app = Arc::clone(&this.inner);
                let result = futures::executor::block_on(async {
                    let mut app = app.lock().await;
                    app.update_note_title(&note_id, &title).await
                });
                result.map_err(|e| mlua::Error::external(e.to_string()))
            },
        );

        // delete_note(note_id)
        methods.add_method("delete_note", |_, this, note_id: String| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let mut app = app.lock().await;
                app.delete_note(&note_id).await
            });
            result.map_err(|e| mlua::Error::external(e.to_string()))
        });

        // find_note(partial_id) -> LuaNote or nil
        methods.add_method("find_note", |_, this, partial_id: String| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let app = app.lock().await;
                app.find_note(&partial_id).cloned()
            });
            Ok(result.map(LuaNote::new))
        });

        // note_count() -> int
        methods.add_method("note_count", |_, this, ()| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let app = app.lock().await;
                app.current_notebook().map(|nb| nb.count()).unwrap_or(0)
            });
            Ok(result)
        });

        // get_invite() -> invite_string
        methods.add_method("get_invite", |_, this, ()| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let app = app.lock().await;
                app.get_invite()
            });
            result.map_err(|e| mlua::Error::external(e.to_string()))
        });

        // join_notebook(invite) -> interface_id_hex
        methods.add_method("join_notebook", |_, this, invite: String| {
            let app = Arc::clone(&this.inner);
            let result = futures::executor::block_on(async {
                let mut app = app.lock().await;
                app.join_notebook(&invite).await
            });
            result
                .map(|id| hex::encode(id.as_bytes()))
                .map_err(|e| mlua::Error::external(e.to_string()))
        });

        // tostring metamethod
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            let app = Arc::clone(&this.inner);
            let desc = futures::executor::block_on(async {
                let app = app.lock().await;
                let name = app.user_name().unwrap_or("uninitialized");
                format!("App({})", name)
            });
            Ok(desc)
        });
    }
}

/// Register App type with the notes table
pub fn register(lua: &Lua, notes: &Table) -> Result<()> {
    let app_table = lua.create_table()?;

    // App.new() -> App (uses default storage location)
    app_table.set(
        "new",
        lua.create_function(|_, ()| {
            let result = futures::executor::block_on(async { App::new().await });
            result
                .map(|app| LuaApp::new(Arc::new(Mutex::new(app))))
                .map_err(|e| mlua::Error::external(e.to_string()))
        })?,
    )?;

    // App.new_with_temp_storage() -> App (uses temp directory for isolated testing)
    app_table.set(
        "new_with_temp_storage",
        lua.create_function(|_, ()| {
            let result = futures::executor::block_on(async {
                let temp_dir = tempfile::TempDir::new()
                    .map_err(|e| crate::app::AppError::Storage(crate::storage::StorageError::Io(e)))?;

                // Store the temp_dir path and create storage
                let storage = LocalStorage::new(temp_dir.path()).await?;
                let app = App::with_storage(storage).await?;

                // We need to keep temp_dir alive - store it alongside the app
                // For simplicity in this implementation, we'll let it leak
                // In production, you'd want proper cleanup
                std::mem::forget(temp_dir);

                Ok(app)
            });
            result
                .map(|app| LuaApp::new(Arc::new(Mutex::new(app))))
                .map_err(|e: crate::app::AppError| mlua::Error::external(e.to_string()))
        })?,
    )?;

    notes.set("App", app_table)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_lua() -> Lua {
        let lua = Lua::new();
        let notes = lua.create_table().unwrap();
        register(&lua, &notes).unwrap();
        super::super::note::register(&lua, &notes).unwrap();
        super::super::notebook::register(&lua, &notes).unwrap();
        lua.globals().set("notes", notes).unwrap();
        lua
    }

    #[tokio::test]
    async fn test_app_with_temp_storage() {
        let lua = setup_lua();

        // Create app with temp storage
        lua.load(
            r#"
            app = notes.App.new_with_temp_storage()
        "#,
        )
        .exec()
        .unwrap();

        // Should not be initialized yet
        let is_init: bool = lua.load("return app:is_initialized()").eval().unwrap();
        assert!(!is_init);
    }

    #[tokio::test]
    async fn test_app_init_and_create_notebook() {
        let lua = setup_lua();

        lua.load(
            r#"
            app = notes.App.new_with_temp_storage()
            app:init("TestUser")
        "#,
        )
        .exec()
        .unwrap();

        let name: String = lua.load("return app:user_name()").eval().unwrap();
        assert_eq!(name, "TestUser");

        // Create notebook
        let nb_id: String = lua
            .load(
                r#"
            return app:create_notebook("Test Notebook")
        "#,
            )
            .eval()
            .unwrap();
        assert!(!nb_id.is_empty());
    }

    #[tokio::test]
    async fn test_app_note_operations() {
        let lua = setup_lua();

        // Setup: create app, init, create notebook, open it
        lua.load(
            r#"
            app = notes.App.new_with_temp_storage()
            app:init("Alice")
            nb_id = app:create_notebook("Notes")
            app:open_notebook(nb_id)
        "#,
        )
        .exec()
        .unwrap();

        // Create a note
        let note_id: String = lua
            .load("return app:create_note('First Note')")
            .eval()
            .unwrap();
        assert!(!note_id.is_empty());

        // Find the note
        lua.globals().set("note_id", note_id.clone()).unwrap();
        let title: String = lua
            .load(
                r#"
            local note = app:find_note(note_id:sub(1, 8))
            return note.title
        "#,
            )
            .eval()
            .unwrap();
        assert_eq!(title, "First Note");

        // Update content
        lua.load(
            r#"
            app:update_note_content(note_id, "Hello world!")
        "#,
        )
        .exec()
        .unwrap();

        let content: String = lua
            .load(
                r#"
            local note = app:find_note(note_id:sub(1, 8))
            return note.content
        "#,
            )
            .eval()
            .unwrap();
        assert_eq!(content, "Hello world!");

        // Note count
        let count: usize = lua.load("return app:note_count()").eval().unwrap();
        assert_eq!(count, 1);

        // Delete
        lua.load("app:delete_note(note_id)").exec().unwrap();
        let count: usize = lua.load("return app:note_count()").eval().unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn test_app_list_notebooks() {
        let lua = setup_lua();

        lua.load(
            r#"
            app = notes.App.new_with_temp_storage()
            app:init("Bob")
            app:create_notebook("Notebook 1")
            app:create_notebook("Notebook 2")
        "#,
        )
        .exec()
        .unwrap();

        let count: usize = lua
            .load(
                r#"
            local notebooks = app:list_notebooks()
            return #notebooks
        "#,
            )
            .eval()
            .unwrap();
        assert_eq!(count, 2);
    }
}

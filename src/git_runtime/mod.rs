//! Git Runtime module
//! 
//! Implements repo-as-room loading via cocapn protocol.

use std::process::Command;
use anyhow::Result;

/// A git repository representing a room in Plato
#[derive(Debug, Clone)]
pub struct Repo {
    pub name: String,
    pub path: String,
    pub remote: String,
}

/// Git Runtime - handles git operations for Plato
pub struct GitRuntime {
    workspace: String,
}

impl GitRuntime {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            workspace: "/tmp/plato-workspace".to_string(),
        })
    }

    /// Checkout/fetch a room repo
    pub async fn checkout(&self, room: &str) -> Result<Repo> {
        let repo_path = format!("{}/{}", self.workspace, room);
        
        // Check if repo exists locally
        let exists = std::path::Path::new(&repo_path).exists();
        
        if exists {
            // Pull latest
            let output = Command::new("git")
                .args(["-C", &repo_path, "pull", "origin", "main"])
                .output()?;
            
            tracing::info!("Pulled {}: {}", room, String::from_utf8_lossy(&output.stderr));
        } else {
            // Clone the repo
            let remote = format!("https://github.com/SuperInstance/{}.git", room);
            let output = Command::new("git")
                .args(["clone", &remote, &repo_path])
                .output()?;
            
            tracing::info!("Cloned {}: {}", room, String::from_utf8_lossy(&output.stderr));
        }
        
        Ok(Repo {
            name: room.to_string(),
            path: repo_path,
            remote: format!("https://github.com/SuperInstance/{}.git", room),
        })
    }

    /// Get the room's constraint file
    pub async fn get_constraints(&self, repo: &Repo) -> Result<String> {
        let constraints_path = format!("{}/.plato/CONSTRAINTS.yaml", repo.path);
        
        if std::path::Path::new(&constraints_path).exists() {
            Ok(std::fs::read_to_string(&constraints_path)?)
        } else {
            // Return default constraints
            Ok("constraints: []".to_string())
        }
    }

    /// Get room description from ROOM.md
    pub async fn get_room_description(&self, repo: &Repo) -> Result<String> {
        let room_path = format!("{}/ROOM.md", repo.path);
        
        if std::path::Path::new(&room_path).exists() {
            Ok(std::fs::read_to_string(&room_path)?)
        } else {
            Ok(format!("# {}\n\nA room in the Plato MUD.", repo.name))
        }
    }
}

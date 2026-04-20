//! Git Runtime module
//! 
//! Implements repo-as-room loading via cocapn protocol with fleet-wide discovery.

use std::process::Command;
use serde::Deserialize;
use anyhow::Result;

/// Fleet index entry from the Agora's plato-index.yaml
#[derive(Debug, Clone, Deserialize)]
pub struct FleetRoom {
    pub repo: String,
    #[serde(rename = "type")]
    pub room_type: String,
    pub agents: Vec<String>,
    pub constraints: Vec<String>,
}

/// Fleet index from the Agora meta-repo
#[derive(Debug, Clone, Deserialize)]
pub struct FleetIndex {
    pub rooms: Vec<FleetRoom>,
}

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
    agora_path: Option<String>,
    fleet_index: Option<FleetIndex>,
}

impl GitRuntime {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            workspace: "/tmp/plato-workspace".to_string(),
            agora_path: None,
            fleet_index: None,
        })
    }

    /// Join the fleet by checking out the Agora (meta-repo) and loading the fleet index
    pub async fn join_fleet(&mut self, agora_remote: &str) -> Result<()> {
        let agora_path = format!("{}/agora", self.workspace);
        
        // Check if Agora exists locally, clone or pull
        let exists = std::path::Path::new(&agora_path).exists();
        if exists {
            let output = Command::new("git")
                .args(["-C", &agora_path, "pull", "origin", "main"])
                .output()?;
            tracing::info!("Pulled Agora: {}", String::from_utf8_lossy(&output.stderr));
        } else {
            let output = Command::new("git")
                .args(["clone", agora_remote, &agora_path])
                .output()?;
            tracing::info!("Cloned Agora: {}", String::from_utf8_lossy(&output.stderr));
        }
        
        // Load and parse the fleet index
        let index_path = format!("{}/plato-index.yaml", agora_path);
        if std::path::Path::new(&index_path).exists() {
            let index_content = std::fs::read_to_string(&index_path)?;
            let fleet_index: FleetIndex = serde_yaml::from_str(&index_content)?;
            self.fleet_index = Some(fleet_index);
            tracing::info!("Loaded fleet index with {} rooms", self.fleet_index.as_ref().map_or(0, |f| f.rooms.len()));
        } else {
            tracing::warn!("Agora missing plato-index.yaml, fleet discovery disabled");
        }
        
        self.agora_path = Some(agora_path);
        Ok(())
    }

    /// List all available rooms in the fleet
    pub async fn list_fleet_rooms(&self) -> Result<&[FleetRoom]> {
        match &self.fleet_index {
            Some(index) => Ok(&index.rooms),
            None => Err(anyhow::anyhow!("Not joined to a fleet, call join_fleet first"))
        }
    }

    /// Checkout/fetch a room repo by name, using fleet index if available
    pub async fn checkout(&self, room: &str) -> Result<Repo> {
        // Try to find the room in the fleet index first
        let remote = if let Some(index) = &self.fleet_index {
            index.rooms.iter()
                .find(|r| r.repo.ends_with(&format!("/{}", room)) || r.repo == room)
                .map(|r| format!("https://{}.git", r.repo))
                .unwrap_or_else(|| format!("https://github.com/SuperInstance/{}.git", room))
        } else {
            format!("https://github.com/SuperInstance/{}.git", room)
        };

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
            let output = Command::new("git")
                .args(["clone", &remote, &repo_path])
                .output()?;
            
            tracing::info!("Cloned {} from {}: {}", room, remote, String::from_utf8_lossy(&output.stderr));
        }
        
        Ok(Repo {
            name: room.to_string(),
            path: repo_path,
            remote,
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

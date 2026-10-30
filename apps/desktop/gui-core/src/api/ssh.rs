use super::app::App;
use super::display::summary;
use super::dto::*;
use anyhow::Result;
use serde::Serialize;

// ---------------- SSH keys (v0.7)
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SshKeyCreated { pub item: ItemSummary, pub public_key: String, pub fingerprint: String, pub key_type: String }
#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SshAgentInfo { pub socket_path: String, pub config_line: String, pub running: bool, pub approve_each_use: bool }


impl App {
    pub(crate) fn save_ssh_key(&self, vault_id: &str, title: &str, k: &gitpass_core::ssh::SshKey) -> Result<SshKeyCreated> {
        let title = if title.trim().is_empty() { if k.info.comment.is_empty() { "SSH Key".to_string() } else { k.info.comment.clone() } } else { title.trim().to_string() };
        let rec = k.to_record(&title)?;
        let item = self.write(|u| {
            let iid = u.session.add_record(vault_id, rec)?;
            let (_, r) = u.session.read_record(vault_id, &iid)?;
            Ok(summary(vault_id, &iid, &r, None))
        })?;
        Ok(SshKeyCreated { item, public_key: k.info.public_key.clone(), fingerprint: k.info.fingerprint.clone(), key_type: k.info.key_type.clone() })
    }
    /// Generate an Ed25519 key into a new `ssh_key` item.
    pub fn ssh_generate(&self, vault_id: &str, title: &str, comment: &str) -> Result<SshKeyCreated> {
        let k = gitpass_core::ssh::SshKey::generate_ed25519(comment)?;
        self.save_ssh_key(vault_id, title, &k)
    }
    /// Import an OpenSSH private key (decrypted with `passphrase` if needed) into a new `ssh_key` item.
    pub fn ssh_import(&self, vault_id: &str, title: &str, pem: &str, passphrase: Option<&str>) -> Result<SshKeyCreated> {
        let k = gitpass_core::ssh::SshKey::import_openssh(pem, passphrase)?;
        self.save_ssh_key(vault_id, title, &k)
    }
    pub fn ssh_agent_info(&self, socket: &std::path::Path, running: bool) -> SshAgentInfo {
        SshAgentInfo { socket_path: socket.display().to_string(), config_line: gitpass_core::ssh::identity_agent_line(socket), running, approve_each_use: self.get_settings().ssh_approve_each_use }
    }
    /// Sign requests handled by the SSH agent since unlock, newest first (metadata only, max 50).
    pub fn ssh_agent_activity(&self) -> Vec<crate::ssh_activity::SshActivityEntry> {
        self.ssh_activity.entries()
    }
    pub fn clear_ssh_agent_activity(&self) {
        self.ssh_activity.clear();
    }


    /// Get the Git commit signing configuration for an SSH key item.
    pub fn ssh_git_signing_config(&self, vault_id: &str, item_id: &str, email: Option<&str>, socket: Option<&str>) -> Result<gitpass_core::ssh::GitSigningConfig> {
        let (_, r) = self.with(|u| u.session.read_record(vault_id, item_id))?;
        let k = gitpass_core::ssh::SshKey::from_record(&r)?;
        let signers_path = gitpass_core::ssh::default_allowed_signers_path();
        let socket_path = socket.map(std::path::PathBuf::from).unwrap_or_else(gitpass_core::ssh::default_ssh_agent_socket_path);
        let email_val = gitpass_core::ssh::resolve_signing_email(email, &k);
        gitpass_core::ssh::build_git_signing_config(&k, &email_val, &signers_path, &socket_path)
    }
    /// Apply the Git commit signing configuration for an SSH key item.
    pub fn ssh_apply_git_signing(&self, vault_id: &str, item_id: &str, email: Option<&str>, allowed_signers_file: Option<&str>, socket: Option<&str>, global: bool) -> Result<gitpass_core::ssh::GitSigningConfig> {
        let (_, r) = self.with(|u| u.session.read_record(vault_id, item_id))?;
        let k = gitpass_core::ssh::SshKey::from_record(&r)?;
        let signers_path = allowed_signers_file
            .map(std::path::PathBuf::from)
            .unwrap_or_else(gitpass_core::ssh::default_allowed_signers_path);
        let socket_path = socket.map(std::path::PathBuf::from).unwrap_or_else(gitpass_core::ssh::default_ssh_agent_socket_path);
        let email_val = gitpass_core::ssh::resolve_signing_email(email, &k);
        gitpass_core::ssh::apply_git_signing(&k, &email_val, &signers_path, &socket_path, global)
    }

}

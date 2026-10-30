use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use gitpass_core::crypto::{generate_password, KdfParams, SecretKey};
use gitpass_core::item::{Category, FieldKind, Section};
use std::io::Read;
use gitpass_core::merge::ItemRecord;
use gitpass_core::store::{self, Item, Session, UnlockOpts};
use gitpass_pasteboard::PasteboardMarkers;
use serde::Serialize;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(name = "gitpass", version, about = "Zero-knowledge password manager synced via a private git repo")]
struct Cli {
    /// Vault directory (default ~/.gitpass, or $GITPASS_PATH)
    #[arg(long, global = true, env = "GITPASS_PATH")]
    path: Option<PathBuf>,
    /// Accept a failed integrity/rollback check and re-seal the manifest from what is on disk (dangerous)
    #[arg(long, global = true)]
    accept_rollback: bool,
    /// Answer yes to confirmation prompts (v1 migration, share, revoke, plaintext export)
    #[arg(long, global = true)]
    yes: bool,
    /// Name to register this device under (default: $GITPASS_DEVICE_NAME, else hostname)
    #[arg(long, global = true, env = "GITPASS_DEVICE_NAME")]
    device_name: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum AddCategory {
    #[default]
    Login,
    ApiCredential,
    SecureNote,
    CreditCard,
    BankAccount,
    Server,
    Identity,
    Wifi,
    SshKey,
}

impl clap::ValueEnum for AddCategory {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::Login,
            Self::ApiCredential,
            Self::SecureNote,
            Self::CreditCard,
            Self::BankAccount,
            Self::Server,
            Self::Identity,
            Self::Wifi,
            Self::SshKey,
        ]
    }
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Self::Login => clap::builder::PossibleValue::new("login"),
            Self::ApiCredential => clap::builder::PossibleValue::new("api-credential").alias("api_credential"),
            Self::SecureNote => clap::builder::PossibleValue::new("secure-note").alias("secure_note"),
            Self::CreditCard => clap::builder::PossibleValue::new("credit-card").alias("credit_card"),
            Self::BankAccount => clap::builder::PossibleValue::new("bank-account").alias("bank_account"),
            Self::Server => clap::builder::PossibleValue::new("server"),
            Self::Identity => clap::builder::PossibleValue::new("identity"),
            Self::Wifi => clap::builder::PossibleValue::new("wifi"),
            Self::SshKey => clap::builder::PossibleValue::new("ssh-key").alias("ssh_key"),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CliCategory {
    Login,
    CreditCard,
    SecureNote,
    Identity,
    ApiCredential,
    Document,
    SshKey,
    Wifi,
    Passkey,
    BankAccount,
    Server,
}

impl clap::ValueEnum for CliCategory {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::Login,
            Self::CreditCard,
            Self::SecureNote,
            Self::Identity,
            Self::ApiCredential,
            Self::Document,
            Self::SshKey,
            Self::Wifi,
            Self::Passkey,
            Self::BankAccount,
            Self::Server,
        ]
    }
    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Self::Login => clap::builder::PossibleValue::new("login"),
            Self::CreditCard => clap::builder::PossibleValue::new("credit-card").alias("credit_card"),
            Self::SecureNote => clap::builder::PossibleValue::new("secure-note").alias("secure_note"),
            Self::Identity => clap::builder::PossibleValue::new("identity"),
            Self::ApiCredential => clap::builder::PossibleValue::new("api-credential").alias("api_credential"),
            Self::Document => clap::builder::PossibleValue::new("document"),
            Self::SshKey => clap::builder::PossibleValue::new("ssh-key").alias("ssh_key"),
            Self::Wifi => clap::builder::PossibleValue::new("wifi"),
            Self::Passkey => clap::builder::PossibleValue::new("passkey"),
            Self::BankAccount => clap::builder::PossibleValue::new("bank-account").alias("bank_account"),
            Self::Server => clap::builder::PossibleValue::new("server"),
        })
    }
}

impl From<CliCategory> for Category {
    fn from(c: CliCategory) -> Self {
        match c {
            CliCategory::Login => Category::Login,
            CliCategory::CreditCard => Category::CreditCard,
            CliCategory::SecureNote => Category::SecureNote,
            CliCategory::Identity => Category::Identity,
            CliCategory::ApiCredential => Category::ApiCredential,
            CliCategory::Document => Category::Document,
            CliCategory::SshKey => Category::SshKey,
            CliCategory::Wifi => Category::Wifi,
            CliCategory::Passkey => Category::Passkey,
            CliCategory::BankAccount => Category::BankAccount,
            CliCategory::Server => Category::Server,
        }
    }
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum Cmd {
    /// Create a new account: master password + generated Secret Key
    Init {
        /// Also save the Secret Key to <path>/secret.key (convenience; git-ignored)
        #[arg(long)]
        save_secret_key: bool,
    },
    /// Verify master password + Secret Key
    Unlock,
    /// Vault management
    Vault { #[command(subcommand)] cmd: VaultCmd },
    /// Manage attachments on an item
    Attach { #[command(subcommand)] cmd: AttachCmd },
    /// Add an item (login, API credential, secure note, credit card, bank account, server, identity, Wi-Fi, or SSH key)
    #[command(after_help = "\
EXAMPLES:
  # Login (password prompted without echo)
  gitpass add Personal --category login --title \"GitHub\" --username octocat --url https://github.com

  # API Credential (credential prompted without echo)
  gitpass add Personal --category api-credential --title \"OpenAI API\" --username org-acme --expires 2028-06-30

  # Secure Note
  gitpass add Personal --category secure-note --title \"Recovery Codes\" --notes \"code1\\ncode2\"

  # Credit Card (card number prompted without echo)
  gitpass add Personal --category credit-card --title \"Bank Visa\" --holder \"Alice Doe\" --expiry 11/30

  # Bank Account (account number prompted without echo)
  gitpass add Personal --category bank-account --title \"Chase Checking\" --bank \"Chase\" --holder \"Alice Doe\" --account-type checking --routing 021000021

  # Server (password prompted without echo)
  gitpass add Personal --category server --title \"Production Web\" --url 192.168.1.50 --username ubuntu

  # Identity
  gitpass add Personal --category identity --title \"Alice Personal\" --first-name Alice --last-name Doe --email alice@example.com --phone \"+1-555-0100\" --address \"123 Main St, Springfield\"

  # Wi-Fi (password prompted without echo)
  gitpass add Personal --category wifi --title \"Home Wi-Fi\" --ssid \"HomeNet\" --security WPA3

  # SSH Key (generate Ed25519 key or import existing file; private key is never printed)
  gitpass add Personal --category ssh-key --title \"Work Laptop\" --generate ed25519
  gitpass add Personal --category ssh-key --title \"Deploy Key\" --private-key-file ~/.ssh/id_ed25519\
")]
    Add {
        vault: String,
        /// Item category / type: login (default), api-credential, secure-note, credit-card, bank-account, server, identity, wifi, ssh-key
        #[arg(long, alias = "type", default_value = "login")]
        category: AddCategory,
        /// Item title
        #[arg(long)]
        title: String,

        // Login & API Credential fields
        /// Username
        #[arg(long)]
        username: Option<String>,
        /// Website URL or API hostname
        #[arg(long, alias = "host")]
        url: Option<String>,

        // Login specific fields
        /// One-time password (TOTP) seed or otpauth:// URI
        #[arg(long)]
        totp: Option<String>,
        /// Generate a password (optional length, default 24) for login/server, or key type for ssh-key (ed25519)
        #[arg(long, num_args = 0..=1, default_missing_value = "24")]
        generate: Option<String>,
        /// Read login or Wi-Fi password from stdin instead of prompting (secrets are never passed as command-line arguments)
        #[arg(long)]
        password_stdin: bool,

        // API Credential specific fields
        /// Read API credential secret from stdin instead of prompting
        #[arg(long)]
        credential_stdin: bool,
        /// Expiration date for API credential (YYYY-MM-DD)
        #[arg(long)]
        expires: Option<String>,
        /// Hidden alias for prompting
        #[arg(long, hide = true)]
        credential: bool,

        // Secure Note & general notes
        /// Notes (plain text)
        #[arg(long)]
        notes: Option<String>,
        /// Read note text from stdin (multi-line)
        #[arg(long)]
        notes_stdin: bool,

        // Credit Card specific fields
        /// Cardholder name
        #[arg(long)]
        holder: Option<String>,
        /// Card expiration date (MM/YY)
        #[arg(long)]
        expiry: Option<String>,

        // Bank Account specific fields
        /// Bank name for bank-account items
        #[arg(long)]
        bank: Option<String>,
        /// Account type for bank-account items (e.g. checking, savings)
        #[arg(long)]
        account_type: Option<String>,
        /// Routing / SWIFT / IBAN code for bank-account items
        #[arg(long)]
        routing: Option<String>,
        /// Branch phone for bank-account items
        #[arg(long)]
        branch_phone: Option<String>,
        /// Branch address for bank-account items
        #[arg(long)]
        branch_address: Option<String>,
        /// Read account number from stdin for bank-account items
        #[arg(long)]
        account_number_stdin: bool,
        /// Read PIN from stdin for bank-account items
        #[arg(long)]
        pin_stdin: bool,

        // Server specific fields
        /// Admin console URL for server items
        #[arg(long)]
        admin_url: Option<String>,
        /// Hosting provider name for server items
        #[arg(long)]
        provider: Option<String>,

        // Identity specific fields
        /// First name for identity items
        #[arg(long)]
        first_name: Option<String>,
        /// Last name for identity items
        #[arg(long)]
        last_name: Option<String>,
        /// Email address for identity items
        #[arg(long)]
        email: Option<String>,
        /// Phone number for identity items
        #[arg(long)]
        phone: Option<String>,
        /// Physical / postal address for identity items
        #[arg(long)]
        address: Option<String>,

        // Wi-Fi specific fields
        /// Wi-Fi network SSID (network name) for wifi items
        #[arg(long)]
        ssid: Option<String>,
        /// Wi-Fi network security protocol (WPA2, WPA3, WEP, none) for wifi items
        #[arg(long)]
        security: Option<String>,

        // SSH Key specific fields
        /// Path to an OpenSSH private key file to import for ssh-key items (passphrase from $GITPASS_SSH_PASSPHRASE or prompt; private keys are never passed as command-line arguments)
        #[arg(long)]
        private_key_file: Option<PathBuf>,
    },
    /// Decrypt and print an item (query = id, id prefix, or fuzzy match on title/username/url)
    Get {
        vault: String,
        query: String,
        /// Print only the password
        #[arg(long)]
        password_only: bool,
        /// Choose the n-th (1-based) ranked candidate when several match
        #[arg(long)]
        pick: Option<usize>,
        /// Copy the password (or --field) to the clipboard instead of printing the item
        #[arg(long)]
        copy: bool,
        #[arg(long, value_enum, default_value_t = Field::Password)]
        field: Field,
        #[arg(long, default_value_t = 30)]
        clear_secs: u64,
        #[arg(long)]
        quiet: bool,
        /// Output JSON
        #[arg(long)]
        json: bool,
        /// Reveal concealed field values in --json mode
        #[arg(long)]
        reveal: bool,
        /// Include archived and deleted items
        #[arg(long)]
        include_archived: bool,
    },
    /// Copy an item field (default password) to the clipboard; blocks up to --clear-secs, then clears it
    Copy { vault: String, query: String,
        #[arg(long, value_enum, default_value_t = Field::Password)] field: Field,
        #[arg(long)] pick: Option<usize>,
        /// Seconds before the clipboard is cleared (0 = don't wait/clear)
        #[arg(long, default_value_t = 30)] clear_secs: u64,
        /// Never echo the value to stdout, even if the clipboard is unavailable
        #[arg(long)] quiet: bool },
    /// Print the current TOTP code and seconds remaining
    Totp { vault: String, query: String, #[arg(long)] pick: Option<usize>,
        /// Also copy the code to the clipboard (cleared after --clear-secs)
        #[arg(long)] copy: bool,
        #[arg(long, default_value_t = 30)] clear_secs: u64 },
    /// Ranked fuzzy search over title/username/url
    Find {
        vault: String,
        query: String,
        /// Output JSON
        #[arg(long)]
        json: bool,
        /// Include archived and deleted items
        #[arg(long)]
        include_archived: bool,
    },
    /// List items in a vault
    List {
        vault: String,
        /// Fuzzy filter (ranked)
        #[arg(long, short = 'q')]
        query: Option<String>,
        /// Filter by category
        #[arg(long, value_enum, value_name = "CATEGORY")]
        category: Option<CliCategory>,
        /// Sort order: `title` (default) or `recent` (most recently used on this device first).
        /// Ignored when `-q/--query` is given: results are then ranked by match quality.
        #[arg(long, value_enum, value_name = "ORDER")]
        sort: Option<ListSort>,
        /// Output JSON
        #[arg(long)]
        json: bool,
        /// Include archived and deleted items
        #[arg(long)]
        include_archived: bool,
    },
    /// Edit an item
    Edit {
        /// Item query (or vault if item is also given)
        item: String,
        /// Item query when vault is given as first positional argument
        #[arg(hide = true)]
        item2: Option<String>,
        /// Vault containing the item (optional if unambiguous)
        #[arg(long)]
        vault: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        username: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        notes: Option<String>,
        #[arg(long)]
        totp: Option<String>,
        /// Prompt for a new password
        #[arg(long)]
        password: bool,
        #[arg(long, num_args = 0..=1, default_missing_value = "24")]
        generate: Option<usize>,
        /// Add a tag (repeatable)
        #[arg(long = "tag", action = clap::ArgAction::Append)]
        tag: Vec<String>,
        /// Remove a tag (repeatable)
        #[arg(long = "untag", action = clap::ArgAction::Append)]
        untag: Vec<String>,
        /// Mark item as favorite
        #[arg(long, conflicts_with = "unfavorite")]
        favorite: bool,
        /// Unmark item as favorite
        #[arg(long, conflicts_with = "favorite")]
        unfavorite: bool,
        /// Choose the n-th (1-based) ranked candidate when several match
        #[arg(long)]
        pick: Option<usize>,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Archive an item
    Archive {
        /// Item query (or vault if item is also given)
        item: String,
        /// Item query when vault is given as first positional argument
        #[arg(hide = true)]
        item2: Option<String>,
        /// Vault containing the item (optional if unambiguous)
        #[arg(long)]
        vault: Option<String>,
        /// Choose the n-th (1-based) ranked candidate when several match
        #[arg(long)]
        pick: Option<usize>,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Unarchive an item
    Unarchive {
        /// Item query (or vault if item is also given)
        item: String,
        /// Item query when vault is given as first positional argument
        #[arg(hide = true)]
        item2: Option<String>,
        /// Vault containing the item (optional if unambiguous)
        #[arg(long)]
        vault: Option<String>,
        /// Choose the n-th (1-based) ranked candidate when several match
        #[arg(long)]
        pick: Option<usize>,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Restore an item from Recently Deleted
    Undelete {
        /// Item query (or vault if item is also given)
        item: String,
        /// Item query when vault is given as first positional argument
        #[arg(hide = true)]
        item2: Option<String>,
        /// Vault containing the item (optional if unambiguous)
        #[arg(long)]
        vault: Option<String>,
        /// Choose the n-th (1-based) ranked candidate when several match
        #[arg(long)]
        pick: Option<usize>,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Move an item to another vault
    Mv {
        /// Item query (or vault if item is also given)
        item: String,
        /// Item query when vault is given as first positional argument
        #[arg(hide = true)]
        item2: Option<String>,
        /// Source vault containing the item (optional if unambiguous)
        #[arg(long)]
        vault: Option<String>,
        /// Destination vault
        #[arg(long)]
        to: String,
        /// Choose the n-th (1-based) ranked candidate when several match
        #[arg(long)]
        pick: Option<usize>,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Remove an item
    Rm { vault: String, id: String },
    /// Version history of an item (no secrets)
    History {
        /// Item query (id, title, or <vault>/<item>)
        item: String,
        /// Vault containing the item (optional if unambiguous)
        #[arg(long)]
        vault: Option<String>,
        /// Output JSON
        #[arg(long)]
        json: bool,
    },
    /// Restore an item to a previous version
    Restore {
        /// Item query (id, title, or <vault>/<item>)
        item: String,
        /// Prior version number to restore
        #[arg(long)]
        version: u64,
        /// Vault containing the item (optional if unambiguous)
        #[arg(long)]
        vault: Option<String>,
    },
    /// Generate a random password, a memorable passphrase or a PIN
    Gen {
        #[command(subcommand)]
        command: Option<GenSubcommand>,
        /// Do not record the generated password to generator history
        #[arg(long)]
        no_history: bool,
        /// Length of the random password (default 24)
        len: Option<usize>,
        /// Memorable passphrase of N words (3-20) from a 128-word list
        #[arg(long, value_name = "N")]
        words: Option<usize>,
        /// Digits-only PIN of N digits (4-16)
        #[arg(long, value_name = "N")]
        pin: Option<usize>,
        /// Passphrase word separator (default '-' or '.' with --no-symbols)
        #[arg(long, value_name = "CHAR")]
        separator: Option<char>,
        /// Capitalize the first letter of each passphrase word
        #[arg(long)]
        capitalize: bool,
        /// Exclude symbols from random passwords (with --words, use '.' between words)
        #[arg(long)]
        no_symbols: bool,
        /// Exclude digits from random passwords (with --words, drop the passphrase digit)
        #[arg(long)]
        no_digits: bool,
        /// Copy the result to the clipboard as concealed/transient instead of printing it
        #[arg(long)]
        copy: bool,
        /// Seconds before the clipboard is cleared (0 = don't wait/clear)
        #[arg(long, default_value_t = gitpass_cli::gen::DEFAULT_CLEAR_SECS, requires = "copy")]
        clear_secs: u64,
    },
    /// Run a command with secrets from gp://<vault>/<item>/<field> references in inherited environment
    /// variables and/or --env-file injected into its environment only (never written to disk);
    /// resolved values are masked in its stdout/stderr
    Run {
        /// dotenv-style file whose values are (or contain) gp:// references; repeatable (optional; overrides inherited environment variables)
        #[arg(long = "env-file")]
        env_file: Vec<PathBuf>,
        /// Do not mask secrets in the child's output (child then writes straight to the terminal)
        #[arg(long)]
        no_masking: bool,
        /// Command and arguments (after --)
        #[arg(last = true, required = true)]
        cmd: Vec<String>,
    },
    /// Render a template, replacing {{ gp://<vault>/<item>/<field> }} placeholders (stdout by default)
    Inject {
        /// Template file (default: stdin)
        #[arg(long, short = 'i')] input: Option<PathBuf>,
        /// Write to this file (mode 0600) instead of stdout. NOTE: this puts plaintext secrets on disk.
        #[arg(long, short = 'o')] output: Option<PathBuf>,
    },
    /// Print the `gp://<vault>/<item>/<field>` secret reference for an item field (never the secret itself)
    Ref {
        vault: String,
        /// Item id, id prefix, or fuzzy match on title/username/url
        query: String,
        /// Field: password (default), username, url, notes, title, totp, or a custom field label or id
        field: Option<String>,
        /// Choose the n-th (1-based) ranked candidate when several match
        #[arg(long)]
        pick: Option<usize>,
        /// Copy the reference to the clipboard instead of printing it (not a secret: no auto-clear)
        #[arg(long)]
        copy: bool,
    },
    /// Connect the vault folder to a private git remote (GitHub or any git host)
    Remote { #[command(subcommand)] cmd: RemoteCmd },
    /// git add/commit/pull --rebase/push in the vault directory, then verify the manifest
    Sync {
        /// On divergence, field-merge items (per-field clocks) instead of failing on rebase conflicts
        #[arg(long)] merge: bool,
    },
    /// Change the master password (re-wraps keyset.enc only)
    Passwd,
    /// Generate a new Secret Key (re-wraps keyset.enc) and print a new Emergency Kit
    RotateSecretKey {
        /// Overwrite <path>/secret.key with the new key
        #[arg(long)]
        save_secret_key: bool,
    },
    /// Generate a new Account Key and re-wrap all vault keys (recovery from a leaked password+Secret Key)
    RotateAccountKey {
        /// Also rotate every vault key (re-encrypts all items) -- recommended after a real compromise
        #[arg(long)]
        with_vault_keys: bool,
    },
    /// Migrate a format v1 vault directory to the current format
    Migrate,
    /// Device registry: list, rename, revoke (v4: revoke rotates keys)
    Device { #[command(subcommand)] cmd: DeviceCmd },
    /// Account identity (X25519) used for vault sharing
    Identity { #[command(subcommand)] cmd: IdentityCmd },
    /// Import items from another password manager's export file
    Import {
        vault: String,
        /// bitwarden-json | bitwarden-csv | generic-csv | 1pux | 1pif | keepass-csv | chrome-csv | apple-passwords-csv | lastpass-csv | protonpass-csv | gitpass-json
        #[arg(long = "from", alias = "format")] from: Option<String>,
        file: PathBuf,
        /// Parse and count only; write nothing
        #[arg(long)] dry_run: bool,
        /// Import items even if title+username+url already exist
        #[arg(long)] allow_duplicates: bool,
    },
    /// Export a vault (PLAINTEXT unless --encrypted; requires --yes for plaintext)
    Export {
        vault: String,
        #[arg(long, value_enum, default_value_t = ExportFmt::GitpassJson)] format: ExportFmt,
        #[arg(long)] out: PathBuf,
        /// Passphrase-protected gitpass-json (Argon2id + XChaCha20-Poly1305); passphrase from $GITPASS_EXPORT_PASSPHRASE or prompt
        #[arg(long)] encrypted: bool,
    },
    /// SSH keys: generate / import / list / public key, `~/.ssh/config` line, and a standalone agent
    Ssh { #[command(subcommand)] cmd: SshCmd },
    /// Security audit: weak / reused / old / pwned passwords, duplicates, expired or expiring items, insecure URLs, missing TOTP, offline 2FA/passkey availability, unused logins
    ///
    /// Finding kinds: weak (incl. passwords containing the username or site name), reused, old,
    /// pwned (only with --hibp), duplicate, expired, expiring_soon, insecure_url, missing_totp,
    /// totp_available, passkey_available, weak_ssh_key, unused. Findings you ignored in the app are counted but only
    /// listed with --include-ignored. See docs/WATCHTOWER.md.
    Audit {
        /// Vault (default: all vaults)
        vault: Option<String>,
        /// Check passwords against Have I Been Pwned (k-anonymity; only 5 hex chars of SHA-1 are sent)
        #[arg(long)] hibp: bool,
        /// Flag passwords not changed for more than this many days as old (tracks password edits, not general item updates)
        #[arg(long, default_value_t = 365)] max_age_days: u64,
        /// Flag cards, API credentials and other expiry fields that expire within this many days
        #[arg(long, default_value_t = 30)] expiring_days: u64,
        /// Flag logins not used on this device for more than this many months (falls back to the last edit/creation time); 0 turns the check off
        #[arg(long, default_value_t = 12)] unused_months: u64,
        /// Print the report as JSON (counts, per_item and ignored_per_item findings, change_password_url)
        #[arg(long)] json: bool,
        /// Show findings that were ignored by the user
        #[arg(long)] include_ignored: bool,
    },
    /// Generate shell completion scripts for zsh, bash, or fish
    #[command(hide = true, alias = "completion")]
    Completions {
        /// Shell to generate completions for (zsh, bash, fish)
        shell: ShellTarget,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
#[value(rename_all = "lower")]
enum ShellTarget {
    Bash,
    Zsh,
    Fish,
}

impl From<ShellTarget> for clap_complete::Shell {
    fn from(s: ShellTarget) -> Self {
        match s {
            ShellTarget::Bash => clap_complete::Shell::Bash,
            ShellTarget::Zsh => clap_complete::Shell::Zsh,
            ShellTarget::Fish => clap_complete::Shell::Fish,
        }
    }
}

fn print_completions(shell: ShellTarget) {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    clap_complete::generate(
        clap_complete::Shell::from(shell),
        &mut cmd,
        "gitpass",
        &mut std::io::stdout(),
    );
}

#[derive(Clone, Copy, PartialEq, clap::ValueEnum)]
enum ExportFmt { GitpassJson, Csv }

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
enum ListSort { Title, Recent }

struct UreqHibp(ureq::Agent);
impl gitpass_core::audit::PwnedRangeClient for UreqHibp {
    fn range(&self, prefix: &str) -> Result<String> {
        let r = self.0.get(&format!("{}{prefix}", gitpass_core::audit::HIBP_RANGE_URL))
            .set("Add-Padding", "true").set("User-Agent", concat!("gitpass/", env!("CARGO_PKG_VERSION"))).call()
            .map_err(|e| anyhow::anyhow!("HIBP request failed: {e}"))?;
        Ok(r.into_string()?)
    }
}

#[derive(Subcommand)]
enum DeviceCmd {
    /// List devices that have unlocked this account
    List,
    /// Rename a device (id or unique id prefix)
    Rename { id: String, name: String },
    /// Revoke a device: marks it revoked AND rotates the Account Key + every Vault Key (re-encrypts items),
    /// so it cannot read FUTURE changes. Data it already copied cannot be recalled.
    Revoke {
        id: String,
        /// Only set the v3 revocation marker (no key rotation)
        #[arg(long)]
        marker_only: bool,
        /// Also change the master password and generate a new Secret Key (do this if the device knew them)
        #[arg(long)]
        rotate_credentials: bool,
        /// With --rotate-credentials: overwrite <path>/secret.key with the new key
        #[arg(long)]
        save_secret_key: bool,
    },
    /// Clear a revocation marker (run from another, non-revoked device)
    Unrevoke { id: String },
}

#[derive(Subcommand)]
enum RemoteCmd {
    /// git init (if needed), ensure .gitignore, commit the encrypted files and push -u origin main.
    /// URL: https://github.com/YOU/REPO.git or git@github.com:YOU/REPO.git (create an EMPTY private repo first).
    /// Uses your existing git credentials (SSH key or credential helper); never force-pushes.
    Add {
        url: String,
        /// Also accept a local path / file:// URL (testing, local mirrors)
        #[arg(long, hide = true)]
        allow_local: bool,
    },
    /// Show the remote URL, branch and ahead/behind counts
    Show,
    /// Disconnect: remove the 'origin' remote (local files and history are kept; nothing is deleted remotely)
    Remove,
}

#[derive(Subcommand, Debug, Clone)]
enum GenSubcommand {
    /// Inspect or clear generator history
    History {
        #[command(subcommand)]
        action: Option<GenHistoryAction>,
        /// Output JSON array of history entries
        #[arg(long)]
        json: bool,
        /// Reveal the plaintext password (otherwise masked)
        #[arg(long)]
        reveal: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum GenHistoryAction {
    /// Clear all generator history
    Clear,
}

#[derive(Subcommand)]
enum SshCmd {
    /// Generate an Ed25519 key into a new SSH Key item
    Generate { vault: String, #[arg(long)] title: String, /// Key comment (default: the title)
        #[arg(long)] comment: Option<String> },
    /// Import an OpenSSH private key file (passphrase from $GITPASS_SSH_PASSPHRASE or a prompt if encrypted)
    Import { vault: String, file: PathBuf, #[arg(long)] title: Option<String> },
    /// List SSH keys (all vaults, or one)
    List { vault: Option<String> },
    /// Print an SSH key's public key line (for authorized_keys / GitHub)
    Public { vault: String, query: String },
    /// Print the `~/.ssh/config` snippet that points ssh at the gitpass agent
    Config { #[arg(long)] socket: Option<PathBuf> },
    /// Run a foreground SSH agent serving this vault's keys (until Ctrl-C). The desktop app runs its own.
    Agent { #[arg(long)] socket: Option<PathBuf> },
    /// Configure Git commit and tag signing with an SSH key
    GitSigning {
        vault: String,
        query: String,
        /// Path to gitpass SSH agent socket (defaults to standard agent socket)
        #[arg(long)]
        socket: Option<PathBuf>,
        /// Scope configuration to ~/.gitconfig (default)
        #[arg(long, conflicts_with = "local")]
        global: bool,
        /// Scope configuration to the local Git repository
        #[arg(long, conflicts_with = "global")]
        local: bool,
        /// Only print git configuration without modifying files (default)
        #[arg(long)]
        print: bool,
        /// Write configuration to git config and allowed_signers
        #[arg(long, alias = "write")]
        apply: bool,
        /// Skip confirmation prompt when applying configuration
        #[arg(short = 'y', long)]
        yes: bool,
        /// Committer email for allowed_signers (defaults to git user.email or key comment)
        #[arg(long)]
        email: Option<String>,
        /// Path to allowed_signers file (defaults to ~/.ssh/allowed_signers)
        #[arg(long)]
        allowed_signers: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum AttachCmd {
    /// Add an attachment file to an item
    Add {
        vault: String,
        query: String,
        file: PathBuf,
    },
    /// List attachments on an item
    Ls {
        vault: String,
        query: String,
        /// Output JSON array of attachments
        #[arg(long)]
        json: bool,
    },
    /// Decrypt and save an attachment to disk
    Get {
        vault: String,
        query: String,
        /// Attachment name or id
        target: String,
        /// Destination output file path
        #[arg(short = 'o', long = "out")]
        out: Option<PathBuf>,
        /// Overwrite destination file if it already exists
        #[arg(long)]
        force: bool,
    },
    /// Remove an attachment from an item
    Rm {
        vault: String,
        query: String,
        /// Attachment name or id
        target: String,
    },
}

#[derive(Serialize, Clone, Debug, PartialEq)]
struct CliAttachment {
    id: String,
    name: String,
    size: u64,
    mime: String,
}

#[derive(Subcommand)]
enum IdentityCmd {
    /// Print this account's public key + fingerprint (creates the identity on first use; upgrades to format v4)
    Show,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Field { Password, Username, Url, Totp, Notes, Title }

#[derive(Subcommand)]
enum VaultCmd {
    Create { name: String, #[arg(long)] description: Option<String> },
    List,
    /// New Vault Key; re-encrypt every item in the vault
    Rotate { vault: String },
    /// Seal the vault key to another account's identity (public key file such as identity.pub, or base64)
    Share { vault: String, #[arg(long)] to: String },
    /// Import a vault shared with you (path to vaults/<id>/members/<your-fpr>.enc in the sharer's checkout)
    Accept { share: PathBuf },
    /// Remove a member (fingerprint or prefix) and rotate the vault key
    Unshare { vault: String, member: String },
    /// List members a vault is shared with
    Members { vault: String },
    /// Rename a vault (name or id)
    Rename { vault: String, new_name: String },
    /// Permanently delete a vault and all its items (asks to type the vault name unless --yes)
    #[command(alias = "delete")]
    Rm { vault: String, #[arg(long)] yes: bool },
}

fn root(cli: &Cli) -> Result<PathBuf> {
    match &cli.path {
        Some(p) => Ok(p.clone()),
        None => Ok(dirs::home_dir().context("no home dir")?.join(".gitpass")),
    }
}

fn prompt_secret(env: &str, prompt: &str) -> Result<Zeroizing<String>> {
    if let Ok(v) = std::env::var(env) { return Ok(Zeroizing::new(v)); }
    Ok(Zeroizing::new(rpassword::prompt_password(prompt)?))
}

fn load_secret_key(root: &Path) -> Result<SecretKey> {
    if let Ok(v) = std::env::var("GITPASS_SECRET_KEY") { return SecretKey::parse(&v); }
    let f = root.join("secret.key");
    if f.exists() { return SecretKey::parse(&Zeroizing::new(std::fs::read_to_string(f)?)); }
    SecretKey::parse(&prompt_secret("__none__", "Secret Key: ")?)
}

fn confirm(prompt: &str) -> Result<bool> {
    use std::io::Write;
    print!("{prompt} [y/N] ");
    std::io::stdout().flush()?;
    let mut l = String::new();
    std::io::stdin().read_line(&mut l)?;
    Ok(matches!(l.trim(), "y" | "Y" | "yes"))
}

fn do_migrate(root: &Path, pw: &str, sk: &SecretKey) -> Result<()> {
    let b = gitpass_core::migrate::migrate(root, pw, sk, Default::default())?;
    eprintln!("migrated format v1 vault to the current format (v1 backup: {})", b.display());
    Ok(())
}

fn unlock_full(cli: &Cli, root: &Path) -> Result<(Session, Zeroizing<String>, SecretKey)> {
    let pw = prompt_secret("GITPASS_PASSWORD", "Master password: ")?;
    let sk = load_secret_key(root)?;
    if store::read_format(root)?.version < store::MIN_READ_FORMAT {
        if cli.yes || confirm("This vault uses format v1. Migrate to the current format now (a backup is made first)?")? {
            do_migrate(root, &pw, &sk)?;
        } else {
            bail!("format v1 vault: run `gitpass migrate`");
        }
    }
    let opts = UnlockOpts { accept_rollback: cli.accept_rollback, ..Default::default() };
    let s = Session::unlock_device(root, &pw, &sk, &opts, cli.device_name.as_deref())?;
    if s.device_revoked { eprintln!("WARNING: this device ({}) is REVOKED; only `device list` and `rotate-account-key` are allowed", s.device_id); }
    for p in &s.accepted_problems { eprintln!("WARNING (accepted via --accept-rollback): {p}"); }
    if s.kdf_upgraded { eprintln!("note: KDF parameters upgraded to m={} KiB t={} p={}", s.fmt.kdf.m_kib, s.fmt.kdf.t, s.fmt.kdf.p); }
    Ok((s, pw, sk))
}

fn unlock(cli: &Cli, root: &Path) -> Result<Session> { Ok(unlock_full(cli, root)?.0) }

fn new_password() -> Result<Zeroizing<String>> {
    let pw = prompt_secret("GITPASS_NEW_PASSWORD", "New master password: ")?;
    if std::env::var("GITPASS_NEW_PASSWORD").is_err() {
        let pw2 = prompt_secret("__none__", "Confirm new master password: ")?;
        if *pw != *pw2 { bail!("passwords do not match"); }
    }
    if pw.len() < 8 { bail!("master password must be at least 8 characters"); }
    Ok(pw)
}

fn save_sk(root: &Path, enc: &str) -> Result<()> {
    gitpass_core::fsutil::atomic_write(&root.join("secret.key"), format!("{enc}\n").as_bytes())?;
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(root.join("secret.key"), std::fs::Permissions::from_mode(0o600))?; }
    Ok(())
}

fn print_kit(root: &Path, enc: &str) {
    println!("==================== GITPASS EMERGENCY KIT ====================");
    println!("Vault path : {}", root.display());
    println!("Secret Key : {enc}");
    println!("Master pw  : (write it down by hand; never stored)");
    println!("Without BOTH your master password and this Secret Key, your");
    println!("data cannot be recovered. Print this and store it offline.");
    println!("===============================================================");
}

fn validate_date_iso(val: &str) -> Result<()> {
    let parts: Vec<&str> = val.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        bail!("invalid expires date format '{val}', expected YYYY-MM-DD");
    }
    let (_y, m, d): (i32, u32, u32) = match (parts[0].parse(), parts[1].parse(), parts[2].parse()) {
        (Ok(y), Ok(m), Ok(d)) => (y, m, d),
        _ => bail!("invalid expires date format '{val}', expected YYYY-MM-DD"),
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        bail!("invalid expires date '{val}': month must be 01-12, day 01-31");
    }
    Ok(())
}

fn validate_card_expiry(val: &str) -> Result<()> {
    let parts: Vec<&str> = val.split('/').collect();
    if parts.len() != 2 {
        bail!("invalid card expiry '{val}', expected MM/YY");
    }
    let (m, _y): (u32, u32) = match (parts[0].parse(), parts[1].parse()) {
        (Ok(m), Ok(y)) => (m, y),
        _ => bail!("invalid card expiry '{val}', expected MM/YY"),
    };
    if !(1..=12).contains(&m) {
        bail!("invalid card expiry month in '{val}', expected 01-12");
    }
    if !(parts[1].len() == 2 || parts[1].len() == 4) {
        bail!("invalid card expiry year in '{val}', expected 2 or 4 digits (MM/YY)");
    }
    Ok(())
}

struct AddFlags<'a> {
    category: AddCategory,
    username: &'a Option<String>,
    url: &'a Option<String>,
    totp: &'a Option<String>,
    generate: &'a Option<String>,
    password_stdin: bool,
    credential_stdin: bool,
    expires: &'a Option<String>,
    notes: &'a Option<String>,
    notes_stdin: bool,
    holder: &'a Option<String>,
    expiry: &'a Option<String>,
    bank: &'a Option<String>,
    account_type: &'a Option<String>,
    routing: &'a Option<String>,
    branch_phone: &'a Option<String>,
    branch_address: &'a Option<String>,
    account_number_stdin: bool,
    pin_stdin: bool,
    admin_url: &'a Option<String>,
    provider: &'a Option<String>,
    first_name: &'a Option<String>,
    last_name: &'a Option<String>,
    email: &'a Option<String>,
    phone: &'a Option<String>,
    address: &'a Option<String>,
    ssid: &'a Option<String>,
    security: &'a Option<String>,
    private_key_file: &'a Option<PathBuf>,
}

fn check_no_identity(flags: &AddFlags) -> Result<()> {
    if flags.first_name.is_some()
        || flags.last_name.is_some()
        || flags.email.is_some()
        || flags.phone.is_some()
        || flags.address.is_some()
    {
        bail!("identity flags are only valid for identity items");
    }
    Ok(())
}

fn check_no_wifi(flags: &AddFlags) -> Result<()> {
    if flags.ssid.is_some() || flags.security.is_some() {
        bail!("wifi flags are only valid for wifi items");
    }
    Ok(())
}

fn check_no_ssh(flags: &AddFlags) -> Result<()> {
    if flags.private_key_file.is_some() {
        bail!("--private-key-file is only valid for ssh-key items");
    }
    Ok(())
}

fn validate_add_flags(flags: &AddFlags) -> Result<()> {
    match flags.category {
        AddCategory::Login => {
            if flags.password_stdin && flags.generate.is_some() {
                bail!("cannot combine --generate and --password-stdin");
            }
            if let Some(g) = flags.generate {
                if g.parse::<usize>().is_err() {
                    bail!("invalid password length '{g}', expected a number");
                }
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is only valid for api-credential items");
            }
            if flags.expires.is_some() {
                bail!("--expires is only valid for api-credential items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.holder.is_some() {
                bail!("--holder is only valid for credit-card items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is only valid for credit-card items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::ApiCredential => {
            if flags.totp.is_some() {
                bail!("--totp is only valid for login items");
            }
            if flags.generate.is_some() {
                bail!("--generate is only valid for login, server, and ssh-key items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is only valid for login and server items (use --credential-stdin for API credentials)");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.holder.is_some() {
                bail!("--holder is only valid for credit-card items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is only valid for credit-card items (use --expires for API credentials)");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
            if let Some(exp) = flags.expires {
                validate_date_iso(exp)?;
            }
        }
        AddCategory::SecureNote => {
            if flags.username.is_some() {
                bail!("--username is not supported for secure-note items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for secure-note items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for secure-note items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for secure-note items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for secure-note items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for secure-note items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for secure-note items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for secure-note items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            if flags.notes.is_some() && flags.notes_stdin {
                bail!("cannot combine --notes and --notes-stdin");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::CreditCard => {
            if flags.username.is_some() {
                bail!("--username is not supported for credit-card items (use --holder for cardholder name)");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for credit-card items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for credit-card items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for credit-card items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for credit-card items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for credit-card items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is not supported for credit-card items");
            }
            if flags.expires.is_some() {
                bail!("--expires is only valid for api-credential items (use --expiry for credit cards)");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
            if let Some(exp) = flags.expiry {
                validate_card_expiry(exp)?;
            }
        }
        AddCategory::BankAccount => {
            if flags.username.is_some() {
                bail!("--username is not supported for bank-account items (use --holder for account holder)");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for bank-account items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for bank-account items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for bank-account items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for bank-account items (use --account-number-stdin or --pin-stdin)");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for bank-account items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is not supported for bank-account items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for bank-account items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::Server => {
            if flags.password_stdin && flags.generate.is_some() {
                bail!("cannot combine --generate and --password-stdin");
            }
            if let Some(g) = flags.generate {
                if g.parse::<usize>().is_err() {
                    bail!("invalid password length '{g}', expected a number");
                }
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for server items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is only valid for api-credential items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is only valid for api-credential items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for server items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for server items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::Identity => {
            if flags.username.is_some() {
                bail!("--username is not supported for identity items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for identity items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for identity items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for identity items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for identity items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for identity items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for identity items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for identity items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for identity items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_wifi(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::Wifi => {
            if flags.ssid.as_deref().unwrap_or("").trim().is_empty() {
                bail!("missing required field: --ssid is required for wifi items");
            }
            if let Some(sec) = flags.security.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                match sec.to_ascii_lowercase().as_str() {
                    "wpa2" | "wpa3" | "wep" | "none" | "wpa" | "wpa/wpa2" | "wpa2/wpa3" => {},
                    _ => bail!("invalid wifi security '{sec}', expected WPA2, WPA3, WEP, or none"),
                }
            }
            if flags.username.is_some() {
                bail!("--username is not supported for wifi items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for wifi items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for wifi items");
            }
            if flags.generate.is_some() {
                bail!("--generate is not supported for wifi items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for wifi items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for wifi items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for wifi items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for wifi items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_ssh(flags)?;
        }
        AddCategory::SshKey => {
            if flags.generate.is_some() && flags.private_key_file.is_some() {
                bail!("cannot combine --generate and --private-key-file");
            }
            if flags.generate.is_none() && flags.private_key_file.is_none() {
                bail!("missing required option for ssh-key items: specify either --generate ed25519 or --private-key-file <PATH>");
            }
            if let Some(g) = flags.generate {
                let g_norm = g.trim().to_ascii_lowercase();
                if g_norm != "ed25519" && g != "24" {
                    bail!("unsupported key type '{g}' for --generate: gitpass currently supports generating 'ed25519' SSH keys");
                }
            }
            if flags.username.is_some() {
                bail!("--username is not supported for ssh-key items");
            }
            if flags.url.is_some() {
                bail!("--url/--host is not supported for ssh-key items");
            }
            if flags.totp.is_some() {
                bail!("--totp is not supported for ssh-key items");
            }
            if flags.password_stdin {
                bail!("--password-stdin is not supported for ssh-key items");
            }
            if flags.credential_stdin {
                bail!("--credential-stdin is not supported for ssh-key items");
            }
            if flags.notes_stdin {
                bail!("--notes-stdin is only valid for secure-note items");
            }
            if flags.expires.is_some() {
                bail!("--expires is not supported for ssh-key items");
            }
            if flags.holder.is_some() {
                bail!("--holder is not supported for ssh-key items");
            }
            if flags.expiry.is_some() {
                bail!("--expiry is not supported for ssh-key items");
            }
            if flags.bank.is_some() || flags.account_type.is_some() || flags.routing.is_some()
                || flags.branch_phone.is_some() || flags.branch_address.is_some()
                || flags.account_number_stdin || flags.pin_stdin {
                bail!("bank account flags are only valid for bank-account items");
            }
            if flags.admin_url.is_some() || flags.provider.is_some() {
                bail!("--admin-url and --provider are only valid for server items");
            }
            check_no_identity(flags)?;
            check_no_wifi(flags)?;
        }
    }
    Ok(())
}
fn print_item(id: &str, r: &ItemRecord, reveal: bool) {
    let it = &r.item;
    println!("id:       {id}");
    println!("title:    {}", it.title);
    if r.category != Category::Login {
        println!("category: {}", r.category.as_str());
    }
    if r.category == Category::Login {
        if let Some(v) = &it.username { println!("username: {v}"); }
        if let Some(v) = &it.password {
            let p = if reveal { v.as_str() } else { "••••••••" };
            println!("password: {p}");
            if let Some(t) = r.password_changed_at() {
                println!("password changed: {}", gitpass_core::device::iso8601(t as i64));
            }
        }
        if let Some(v) = &it.url { println!("url:      {v}"); }
        if let Some(v) = &it.totp {
            let t = if reveal { v.as_str() } else { "••••••••" };
            println!("totp:     {t}");
        }
        if let Some(v) = &it.notes { println!("notes:    {v}"); }
        for s in &r.sections {
            for f in &s.fields {
                if f.removed { continue; }
                if let Some(v) = &f.value {
                    let val = if f.kind.is_secret() {
                        if reveal { v.as_str() } else { "••••••••" }
                    } else {
                        v.as_str()
                    };
                    println!("{}: {val}", f.label);
                }
            }
        }
    } else {
        for s in r.view_sections() {
            if let Some(t) = &s.title {
                println!("[{t}]");
            }
            for f in s.fields {
                if let Some(v) = &f.value {
                    let val = if f.kind.is_secret() {
                        if reveal { v.as_str() } else { "••••••••" }
                    } else {
                        v.as_str()
                    };
                    println!("{}: {val}", f.label);
                }
            }
        }
        if let Some(v) = &it.url {
            let has_url = r.view_sections().iter().any(|s| s.fields.iter().any(|f| f.kind == FieldKind::Url));
            if !has_url {
                println!("url:      {v}");
            }
        }
        if let Some(v) = &it.notes {
            let has_note = r.view_sections().iter().any(|s| s.fields.iter().any(|f| f.id == "note" || f.id == "notes"));
            if !has_note {
                println!("notes:    {v}");
            }
        }
    }
}

fn field_value(it: &Item, f: Field) -> Result<Zeroizing<String>> {
    let v = match f {
        Field::Password => it.password.clone(), Field::Username => it.username.clone(), Field::Url => it.url.clone(),
        Field::Notes => it.notes.clone(), Field::Title => Some(it.title.clone()),
        Field::Totp => match &it.totp { Some(t) => Some(gitpass_core::totp::code_now(&gitpass_core::totp::parse(t)?).0), None => None },
    };
    v.map(Zeroizing::new).ok_or_else(|| anyhow::anyhow!("item '{}' has no such field", it.title))
}

#[derive(Serialize, Clone, Debug, PartialEq)]
struct CliItemSummary {
    id: String,
    title: String,
    category: Category,
    username: Option<String>,
    url: Option<String>,
    tags: Vec<String>,
    favorite: bool,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_used: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    subtitle: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
struct CliField {
    label: String,
    kind: FieldKind,
    value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    masked: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
struct CliItemDetail {
    id: String,
    title: String,
    category: Category,
    username: Option<String>,
    url: Option<String>,
    tags: Vec<String>,
    favorite: bool,
    archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    deleted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    created_at: Option<String>,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    password_changed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    fields: Vec<CliField>,
}

fn ts(s: &str) -> String {
    s.trim().parse::<i64>().map(gitpass_core::device::iso8601).unwrap_or_else(|_| s.to_string())
}

#[derive(Serialize, Clone, Debug, PartialEq)]
struct CliHistoryEntry {
    version: u64,
    updated_at: String,
    current: bool,
}

/// Default output file name for `attach get` without `-o`: only the final path component of the
/// stored name (which comes from synced vault data and must not be trusted), so names like
/// `../../.ssh/authorized_keys` or `/etc/passwd` can never write outside the current directory.
fn safe_attachment_filename(name: &str) -> PathBuf {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    match std::path::Path::new(base).file_name() {
        Some(f) if !f.is_empty() => PathBuf::from(f),
        _ => PathBuf::from("attachment"),
    }
}

fn find_attachment<'a>(attachments: &'a [gitpass_core::item::AttachmentMeta], target: &str) -> Result<&'a gitpass_core::item::AttachmentMeta> {
    if let Some(a) = attachments.iter().find(|a| a.id == target) {
        return Ok(a);
    }
    let by_name: Vec<_> = attachments.iter().filter(|a| a.name == target).collect();
    match by_name.len() {
        1 => Ok(by_name[0]),
        0 => {
            let by_prefix: Vec<_> = attachments.iter().filter(|a| a.id.starts_with(target)).collect();
            match by_prefix.len() {
                1 => Ok(by_prefix[0]),
                0 => bail!("no attachment matches '{target}'"),
                n => bail!("{n} attachments match prefix '{target}'; use the full attachment id"),
            }
        }
        n => bail!("{n} attachments named '{target}'; specify the attachment id"),
    }
}

fn resolve_item_across_vaults(s: &Session, vault: Option<&str>, item: &str) -> Result<(String, String)> {
    if let Some(v) = vault {
        let vid = s.resolve_vault(v)?;
        let iid = s.resolve_item(&vid, item)?;
        return Ok((vid, iid));
    }
    if let Some((v, q)) = item.split_once('/') {
        let vid = s.resolve_vault(v)?;
        let iid = s.resolve_item(&vid, q)?;
        return Ok((vid, iid));
    }
    let mut matches = Vec::new();
    for (vid, _) in s.list_vaults()? {
        if let Ok(m) = s.find_items(&vid, item) {
            for (iid, it) in m {
                matches.push((vid.clone(), iid, it.title.clone()));
            }
        }
    }
    match matches.len() {
        0 => bail!("no item matches '{item}'"),
        1 => Ok((matches[0].0.clone(), matches[0].1.clone())),
        _ => bail!("multiple items match '{item}'; specify --vault or <vault>/<item>"),
    }
}

fn item_summary(id: &str, r: &ItemRecord) -> CliItemSummary {
    let hint = if r.category == Category::ApiCredential {
        gitpass_core::mask::api_credential_secret(r).map(gitpass_core::mask::secret_hint)
    } else {
        None
    };
    CliItemSummary {
        id: id.to_string(),
        title: r.item.title.clone(),
        category: r.category,
        username: r.item.username.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| f.id == "username" && f.kind == FieldKind::Text)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        url: r.item.url.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| (f.id == "hostname" || f.id == "url") && f.kind == FieldKind::Url)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        tags: r.tags.clone(),
        favorite: r.favorite,
        updated_at: ts(&r.item.updated_at),
        last_used: None,
        subtitle: hint.clone(),
        hint,
    }
}

fn item_detail(id: &str, r: &ItemRecord, reveal: bool, last_used_at: Option<String>) -> CliItemDetail {
    let mut fields = Vec::new();
    for section in r.view_sections() {
        for f in section.fields {
            let (val, masked) = if f.kind.is_secret() {
                if reveal {
                    (f.value, None)
                } else if r.category == Category::ApiCredential {
                    (None, f.value.as_deref().map(gitpass_core::mask::secret_hint))
                } else {
                    (None, None)
                }
            } else {
                (f.value, None)
            };
            fields.push(CliField {
                label: f.label,
                kind: f.kind,
                value: val,
                masked,
            });
        }
    }
    CliItemDetail {
        id: id.to_string(),
        title: r.item.title.clone(),
        category: r.category,
        username: r.item.username.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| f.id == "username" && f.kind == FieldKind::Text)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        url: r.item.url.clone().filter(|u| !u.trim().is_empty()).or_else(|| {
            r.view_sections()
                .into_iter()
                .flat_map(|s| s.fields)
                .find(|f| (f.id == "hostname" || f.id == "url") && f.kind == FieldKind::Url)
                .and_then(|f| f.value)
                .filter(|u| !u.trim().is_empty())
        }),
        tags: r.tags.clone(),
        favorite: r.favorite,
        archived: r.archived,
        deleted_at: r.deleted_at.map(|d| gitpass_core::device::iso8601(d as i64)),
        created_at: r.created_at.map(|c| gitpass_core::device::iso8601(c as i64)),
        updated_at: ts(&r.item.updated_at),
        password_changed_at: r.password_changed_at().map(|t| gitpass_core::device::iso8601(t as i64)),
        last_used_at,
        notes: r.item.notes.clone().filter(|n| !n.trim().is_empty()),
        fields,
    }
}

/// Ranked lookup; errors (listing candidates) unless one clear winner or --pick.
fn pick_record(s: &Session, vid: &str, query: &str, pick: Option<usize>, include_archived: bool) -> Result<(String, ItemRecord)> {
    let mut recs = s.list_records(vid)?;
    if !include_archived {
        recs.retain(|(_, _, r)| r.deleted_at.is_none() && !r.archived);
    }
    let vname = s.vault_meta(vid).map(|m| m.name.clone()).unwrap_or_default();
    let ranked = gitpass_core::search::rank_by_candidate(
        query,
        recs.into_iter().map(|(id, _, rec)| (id, rec)),
        |r| gitpass_core::search::CandidateInfo::from_record(r, Some(vname.as_str())),
    );

    if ranked.is_empty() {
        bail!("no item matches '{query}'");
    }
    let idx = match pick {
        Some(n) if n >= 1 && n <= ranked.len() => n - 1,
        Some(n) => bail!("--pick {n} out of range (1..={})", ranked.len()),
        None => match gitpass_core::search::clear_winner(&ranked) {
            Some(i) => i,
            None => {
                for (i, (sc, id, rec)) in ranked.iter().enumerate() {
                    eprintln!("{:>3}. {id}  {}  {}  (score {sc})", i + 1, rec.item.title, rec.item.username.clone().unwrap_or_default());
                }
                bail!("{} items match '{query}'; use --pick <n> or the item id", ranked.len());
            }
        },
    };
    let (_, id, rec) = ranked.into_iter().nth(idx).unwrap();
    Ok((id, rec))
}

fn pick_item(s: &Session, vid: &str, query: &str, pick: Option<usize>) -> Result<(String, Item)> {
    let (id, rec) = pick_record(s, vid, query, pick, false)?;
    Ok((id, rec.item.clone()))
}

fn resolve_target_item(
    s: &Session,
    vault_flag: Option<&str>,
    pos1: &str,
    pos2: Option<&str>,
    pick: Option<usize>,
    include_archived: bool,
) -> Result<(String, String, ItemRecord)> {
    let (vault_str, query_str) = match pos2 {
        Some(q) => (Some(pos1), q),
        None => match pos1.split_once('/') {
            Some((v, q)) => (Some(v), q),
            None => (vault_flag, pos1),
        },
    };

    if let Some(v) = vault_str {
        let vid = s.resolve_vault(v)?;
        let (iid, rec) = pick_record(s, &vid, query_str, pick, include_archived)?;
        return Ok((vid, iid, rec));
    }

    let mut all_recs = Vec::new();
    for (vid, _) in s.list_vaults()? {
        let mut recs = s.list_records(&vid)?;
        if !include_archived {
            recs.retain(|(_, _, r)| r.deleted_at.is_none() && !r.archived);
        }
        for (id, _, rec) in recs {
            all_recs.push((vid.clone(), id, rec));
        }
    }

    let ranked = gitpass_core::search::rank_by(
        query_str,
        all_recs.into_iter().map(|(vid, id, rec)| (format!("{vid}:{id}"), (vid, id, rec))),
        |(_, _, rec)| &rec.item,
    );

    if ranked.is_empty() {
        bail!("no item matches '{query_str}'");
    }

    let idx = match pick {
        Some(n) if n >= 1 && n <= ranked.len() => n - 1,
        Some(n) => bail!("--pick {n} out of range (1..={})", ranked.len()),
        None => match gitpass_core::search::clear_winner(&ranked) {
            Some(i) => i,
            None => {
                for (i, (sc, _, (vid, id, rec))) in ranked.iter().enumerate() {
                    let vname = s.vault_meta(vid).map(|m| m.name.clone()).unwrap_or_else(|_| vid.clone());
                    eprintln!(
                        "{:>3}. [{vname}] {id}  {}  {}  (score {sc})",
                        i + 1,
                        rec.item.title,
                        rec.item.username.clone().unwrap_or_default()
                    );
                }
                bail!("{} items match '{query_str}'; use --pick <n> or specify --vault", ranked.len());
            }
        },
    };

    let (_, _, (vid, id, rec)) = ranked.into_iter().nth(idx).unwrap();
    Ok((vid, id, rec))
}

fn resolve_deleted_item(
    s: &Session,
    vault_flag: Option<&str>,
    pos1: &str,
    pos2: Option<&str>,
    pick: Option<usize>,
) -> Result<(String, String, ItemRecord)> {
    let (vault_str, query_str) = match pos2 {
        Some(q) => (Some(pos1), q),
        None => match pos1.split_once('/') {
            Some((v, q)) => (Some(v), q),
            None => (vault_flag, pos1),
        },
    };

    let search_vaults: Vec<String> = if let Some(v) = vault_str {
        vec![s.resolve_vault(v)?]
    } else {
        s.list_vaults()?.into_iter().map(|(v, _)| v).collect()
    };

    let mut deleted_recs = Vec::new();
    let mut non_deleted_recs = Vec::new();

    for vid in &search_vaults {
        for (id, _, rec) in s.list_records(vid)? {
            if rec.deleted_at.is_some() {
                deleted_recs.push((vid.clone(), id, rec));
            } else {
                non_deleted_recs.push((vid.clone(), id, rec));
            }
        }
    }

    let ranked = gitpass_core::search::rank_by(
        query_str,
        deleted_recs.into_iter().map(|(vid, id, rec)| (format!("{vid}:{id}"), (vid, id, rec))),
        |(_, _, rec)| &rec.item,
    );

    if ranked.is_empty() {
        let active_ranked = gitpass_core::search::rank_by(
            query_str,
            non_deleted_recs.into_iter().map(|(vid, id, rec)| (format!("{vid}:{id}"), (vid, id, rec))),
            |(_, _, rec)| &rec.item,
        );
        if let Some((_, _, (_, _, active_rec))) = active_ranked.into_iter().next() {
            bail!("item '{}' is not deleted", active_rec.item.title);
        }
        bail!("no deleted item matches '{query_str}'");
    }

    let idx = match pick {
        Some(n) if n >= 1 && n <= ranked.len() => n - 1,
        Some(n) => bail!("--pick {n} out of range (1..={})", ranked.len()),
        None => match gitpass_core::search::clear_winner(&ranked) {
            Some(i) => i,
            None => {
                for (i, (sc, _, (vid, id, rec))) in ranked.iter().enumerate() {
                    let vname = s.vault_meta(vid).map(|m| m.name.clone()).unwrap_or_else(|_| vid.clone());
                    eprintln!(
                        "{:>3}. [{vname}] {id}  {}  {}  (score {sc})",
                        i + 1,
                        rec.item.title,
                        rec.item.username.clone().unwrap_or_default()
                    );
                }
                bail!("{} deleted items match '{query_str}'; use --pick <n> or the item id", ranked.len());
            }
        },
    };

    let (_, _, (vid, id, rec)) = ranked.into_iter().nth(idx).unwrap();
    Ok((vid, id, rec))
}

/// Clipboard markers for a copied field: passwords and TOTP codes are secrets; the rest are plain.
fn field_markers(f: Field) -> Option<PasteboardMarkers> {
    match f { Field::Password | Field::Totp => Some(PasteboardMarkers::SECRET), Field::Username | Field::Url | Field::Notes | Field::Title => None }
}

fn do_copy(val: &str, secs: u64, quiet: bool, label: &str, markers: Option<PasteboardMarkers>) -> Result<()> {
    match gitpass_cli::clip::system() {
        Ok(mut cb) => {
            if secs == 0 { gitpass_cli::clip::set_with_markers(cb.as_mut(), val, markers)?; eprintln!("copied {label} to clipboard (not auto-cleared)"); return Ok(()); }
            eprintln!("copied {label} to clipboard; clearing in {secs}s (keep this running, Ctrl-C leaves it)");
            let cleared = gitpass_cli::clip::copy_then_clear(cb.as_mut(), val, markers, std::time::Duration::from_secs(secs), std::thread::sleep)?;
            eprintln!("{}", if cleared { "clipboard cleared" } else { "clipboard changed since copy; left untouched" });
        }
        Err(e) => {
            eprintln!("warning: clipboard unavailable ({e}); {}", if quiet { "value not shown (--quiet)" } else { "printing to stdout" });
            if !quiet { println!("{val}"); }
        }
    }
    Ok(())
}

struct SessionResolver<'a>(&'a Session);
impl gitpass_cli::inject::Resolver for SessionResolver<'_> {
    fn resolve(&self, r: &gitpass_cli::inject::SecretRef) -> Result<Zeroizing<String>> {
        let s = self.0;
        let vid = s.resolve_vault(&r.vault).with_context(|| format!("gp://{}/...: vault", r.vault))?;
        let items = s.list_items(&vid)?;
        let exact: Vec<&(String, Item)> = items.iter().filter(|(id, it)| *id == r.item || it.title.eq_ignore_ascii_case(&r.item)).collect();
        let iid = match exact.len() {
            1 => exact[0].0.clone(),
            0 => s.resolve_item(&vid, &r.item).with_context(|| format!("gp://{}/{}: item", r.vault, r.item))?,
            n => bail!("gp://{}/{}: {n} items have that title; use the item id", r.vault, r.item),
        };
        let (_, rec) = s.read_record(&vid, &iid)?;
        let f = r.field.to_ascii_lowercase();
        let std = match f.as_str() {
            "password" => Some(Field::Password), "username" => Some(Field::Username), "url" => Some(Field::Url),
            "notes" => Some(Field::Notes), "title" => Some(Field::Title), "totp" | "otp" => Some(Field::Totp), _ => None,
        };
        if let Some(sf) = std { return field_value(&rec.item, sf); }
        let hits: Vec<&gitpass_core::item::Field> = rec.sections.iter().flat_map(|sec| sec.fields.iter())
            .filter(|x| !x.removed && (x.label.eq_ignore_ascii_case(&r.field) || x.id == r.field)).collect();
        match hits.as_slice() {
            [one] => one.value.clone().map(Zeroizing::new).ok_or_else(|| anyhow::anyhow!("field '{}' is empty", r.field)),
            [] => bail!("gp://{}/{}/{}: no such field", r.vault, r.item, r.field),
            _ => bail!("gp://{}/{}/{}: several fields have that label; use the field id", r.vault, r.item, r.field),
        }
    }
}

fn main() {
    if let Err(e) = run() { eprintln!("error: {e:#}"); std::process::exit(1); }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Cmd::Completions { shell } = &cli.cmd {
        print_completions(*shell);
        return Ok(());
    }
    let root = root(&cli)?;
    match &cli.cmd {
        Cmd::Init { save_secret_key } => {
            let pw = prompt_secret("GITPASS_PASSWORD", "New master password: ")?;
            if std::env::var("GITPASS_PASSWORD").is_err() {
                let pw2 = prompt_secret("__none__", "Confirm master password: ")?;
                if *pw != *pw2 { bail!("passwords do not match"); }
            }
            if pw.len() < 8 { bail!("master password must be at least 8 characters"); }
            let sk = store::init(&root, &pw, KdfParams::default())?;
            let enc = Zeroizing::new(sk.encode());
            if *save_secret_key { save_sk(&root, &enc)?; }
            print_kit(&root, &enc);
        }
        Cmd::Unlock => {
            let s = unlock(&cli, &root)?;
            { let m = s.load_manifest()?; println!("unlocked OK (keyset format v{}, manifest format v{}, epoch {}, integrity verified)", s.fmt.version, m.format, m.epoch); }
        }
        Cmd::Vault { cmd: VaultCmd::Create { name, description } } => {
            let s = unlock(&cli, &root)?; println!("created vault {name} ({})", s.create_vault_with(name, description.clone())?);
        }
        Cmd::Vault { cmd: VaultCmd::Rename { vault, new_name } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let mut m = s.vault_meta(&vid)?; m.name = new_name.clone();
            s.update_vault_meta(&vid, m)?;
            println!("renamed vault {vault} -> {} ({vid})", new_name.trim());
        }
        Cmd::Vault { cmd: VaultCmd::Rm { vault, yes } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let name = s.vault_meta(&vid)?.name.clone();
            let n = s.list_records(&vid)?.len();
            if !yes {
                eprintln!("This permanently deletes vault '{name}' and its {n} item(s), attachments, history and shares.");
                eprint!("Type the vault name to confirm: ");
                let mut l = String::new(); std::io::stdin().read_line(&mut l)?;
                if l.trim() != name { bail!("name did not match; nothing deleted"); }
            }
            s.delete_vault(&vid)?;
            println!("deleted vault {name} ({vid}, {n} item(s))");
        }
        Cmd::Vault { cmd: VaultCmd::Rotate { vault } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            s.rotate_vault(&vid)?;
            println!("rotated vault key for {vault} ({vid})");
        }
        Cmd::Vault { cmd: VaultCmd::Share { vault, to } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let pk = gitpass_core::keys::load_pub(to)?;
            let fpr = gitpass_core::keys::fingerprint(&pk);
            if !cli.yes && !confirm(&format!("Share vault '{vault}' with fingerprint {fpr}? Verify this fingerprint with the recipient out of band."))? { bail!("aborted"); }
            let p = s.share_vault(&vid, &pk)?;
            println!("shared {vault} ({vid}) with {fpr}");
            println!("member file: {}", p.display());
            eprintln!("Commit + push, then the recipient runs: gitpass vault accept <checkout>/vaults/{vid}/members/{fpr}.enc");
        }
        Cmd::Vault { cmd: VaultCmd::Accept { share } } => {
            let s = unlock(&cli, &root)?;
            if let Ok(b) = std::fs::read(share) {
                if let Ok(mf) = serde_json::from_slice::<store::MemberFile>(&b) {
                    // The signature itself is verified by accept_share below.
                    // The signing key travels inside the file, so on first
                    // contact with a sender, verify this signing fingerprint
                    // out of band before trusting the sender's identity.
                    eprintln!("share claims to be from fingerprint {} (signing fingerprint {}; accept verifies the share signature)", mf.sender_fpr, store::sender_sign_fingerprint(&mf));
                }
            }
            let vid = s.accept_share(share)?;
            println!("imported shared vault {} ({vid}) as a snapshot", s.vault_meta(&vid)?.name);
        }
        Cmd::Vault { cmd: VaultCmd::Unshare { vault, member } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let f = s.unshare_vault(&vid, member)?;
            println!("removed member {f} from {vault}; vault key rotated (they keep anything they already copied)");
        }
        Cmd::Vault { cmd: VaultCmd::Members { vault } } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            for (f, p) in s.members(&vid)? { println!("{f}  {}{p}", gitpass_core::keys::PUB_PREFIX); }
        }
        Cmd::Ssh { cmd } => ssh_cmd(&cli, &root, cmd)?,
        Cmd::Identity { cmd: IdentityCmd::Show } => {
            let s = unlock(&cli, &root)?;
            let (pk, fpr) = s.identity()?;
            println!("public key:  {}", gitpass_core::keys::encode_pub(&pk));
            println!("fingerprint: {fpr}");
            println!("file:        {}", root.join(store::IDENTITY_PUB_FILE).display());
        }
        Cmd::Attach { cmd } => match cmd {
            AttachCmd::Add { vault, query, file } => {
                if !file.exists() {
                    bail!("file '{}' does not exist", file.display());
                }
                let data = zeroize::Zeroizing::new(std::fs::read(file)?);
                if data.len() > gitpass_core::item::MAX_ATTACHMENT_BYTES {
                    bail!("attachment too large ({} bytes exceeds {} MiB limit)", data.len(), gitpass_core::item::MAX_ATTACHMENT_BYTES / (1024 * 1024));
                }
                let s = unlock(&cli, &root)?;
                let vid = s.resolve_vault(vault)?;
                let (iid, rec) = pick_record(&s, &vid, query, None, false)?;
                let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".to_string());
                let mime = gitpass_core::item::mime_for(&name);
                let meta = s.add_attachment(&vid, &iid, &name, mime, &data)?;
                println!("added attachment '{}' ({}, {} bytes) to '{}'", meta.name, meta.id, meta.size, rec.item.title);
            }
            AttachCmd::Ls { vault, query, json } => {
                let s = unlock(&cli, &root)?;
                let vid = s.resolve_vault(vault)?;
                let (_iid, rec) = pick_record(&s, &vid, query, None, false)?;
                if *json {
                    let atts: Vec<CliAttachment> = rec.attachments.iter().map(|a| CliAttachment {
                        id: a.id.clone(),
                        name: a.name.clone(),
                        size: a.size,
                        mime: a.mime.clone(),
                    }).collect();
                    println!("{}", serde_json::to_string_pretty(&atts)?);
                } else if rec.attachments.is_empty() {
                    println!("no attachments");
                } else {
                    for a in &rec.attachments {
                        println!("{:<36}  {:<10}  {}", a.id, a.size, a.name);
                    }
                }
            }
            AttachCmd::Get { vault, query, target, out, force } => {
                let s = unlock(&cli, &root)?;
                let vid = s.resolve_vault(vault)?;
                let (iid, rec) = pick_record(&s, &vid, query, None, false)?;
                let att = find_attachment(&rec.attachments, target)?;
                let dest = out.clone().unwrap_or_else(|| safe_attachment_filename(&att.name));
                if dest.exists() && !*force {
                    bail!("destination '{}' already exists (use --force to overwrite)", dest.display());
                }
                let data = zeroize::Zeroizing::new(s.read_attachment(&vid, &iid, &att.id)?);
                if let Some(p) = dest.parent() {
                    if !p.as_os_str().is_empty() {
                        std::fs::create_dir_all(p)?;
                    }
                }
                std::fs::write(&dest, &*data)?;
                println!("saved attachment '{}' to {}", att.name, dest.display());
            }
            AttachCmd::Rm { vault, query, target } => {
                let s = unlock(&cli, &root)?;
                let vid = s.resolve_vault(vault)?;
                let (iid, rec) = pick_record(&s, &vid, query, None, false)?;
                let att = find_attachment(&rec.attachments, target)?;
                let att_id = att.id.clone();
                let att_name = att.name.clone();
                s.remove_attachment(&vid, &iid, &att_id)?;
                println!("removed attachment '{}' ({}) from '{}'", att_name, att_id, rec.item.title);
            }
        }
        Cmd::Vault { cmd: VaultCmd::List } => {
            let s = unlock(&cli, &root)?;
            for (id, m) in s.list_vaults()? {
                match &m.description { Some(d) => println!("{id}  {}  ({d})", m.name), None => println!("{id}  {}", m.name) }
            }
        }
        Cmd::Add {
            vault,
            category,
            title,
            username,
            url,
            totp,
            generate,
            password_stdin,
            credential_stdin,
            expires,
            notes,
            notes_stdin,
            holder,
            expiry,
            bank,
            account_type,
            routing,
            branch_phone,
            branch_address,
            account_number_stdin,
            pin_stdin,
            admin_url,
            provider,
            first_name,
            last_name,
            email,
            phone,
            address,
            ssid,
            security,
            private_key_file,
            credential: _,
        } => {
            if title.trim().is_empty() {
                bail!("title cannot be empty");
            }
            validate_add_flags(&AddFlags {
                category: *category,
                username,
                url,
                totp,
                generate,
                password_stdin: *password_stdin,
                credential_stdin: *credential_stdin,
                expires,
                notes,
                notes_stdin: *notes_stdin,
                holder,
                expiry,
                bank,
                account_type,
                routing,
                branch_phone,
                branch_address,
                account_number_stdin: *account_number_stdin,
                pin_stdin: *pin_stdin,
                admin_url,
                provider,
                first_name,
                last_name,
                email,
                phone,
                address,
                ssid,
                security,
                private_key_file,
            })?;
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;

            let rec = match category {
                AddCategory::Login => {
                    let password = if *password_stdin {
                        let mut p = String::new();
                        std::io::stdin().read_line(&mut p)?;
                        let p = p.trim_end_matches(['\r', '\n']).to_string();
                        if p.is_empty() { None } else { Some(p) }
                    } else if let Some(n) = generate {
                        let len = n.parse::<usize>().unwrap_or(24);
                        Some(generate_password(len))
                    } else {
                        let p = prompt_secret("GITPASS_ITEM_PASSWORD", "Item password (empty for none): ")?;
                        if p.is_empty() { None } else { Some(p.to_string()) }
                    };
                    let it = Item {
                        title: title.clone(),
                        username: username.clone(),
                        password,
                        url: url.clone(),
                        notes: notes.clone(),
                        totp: totp.clone(),
                        updated_at: store::now(),
                    };
                    ItemRecord::new(it)
                }
                AddCategory::ApiCredential => {
                    let cred_val = if *credential_stdin {
                        let mut c = String::new();
                        std::io::stdin().read_line(&mut c)?;
                        let c = c.trim_end_matches(['\r', '\n']).to_string();
                        if c.is_empty() {
                            bail!("credential from stdin cannot be empty");
                        }
                        c
                    } else {
                        let p = prompt_secret("GITPASS_ITEM_CREDENTIAL", "API credential: ")?;
                        if p.is_empty() {
                            bail!("credential cannot be empty");
                        }
                        p.to_string()
                    };
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: None,
                        url: url.clone(),
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut fields = Vec::new();
                    if let Some(u) = username.as_ref().filter(|u| !u.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "username".into(),
                            label: "username".into(),
                            kind: FieldKind::Text,
                            value: Some(u.clone()),
                            removed: false,
                        });
                    }
                    fields.push(gitpass_core::item::Field {
                        id: "credential".into(),
                        label: "credential".into(),
                        kind: FieldKind::Concealed,
                        value: Some(cred_val),
                        removed: false,
                    });
                    if let Some(u) = url.as_ref().filter(|u| !u.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "hostname".into(),
                            label: "hostname".into(),
                            kind: FieldKind::Url,
                            value: Some(u.clone()),
                            removed: false,
                        });
                    }
                    if let Some(exp) = expires.as_ref().filter(|e| !e.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "expires".into(),
                            label: "expires".into(),
                            kind: FieldKind::Date,
                            value: Some(exp.clone()),
                            removed: false,
                        });
                    }
                    let mut r = ItemRecord::new(it);
                    r.category = Category::ApiCredential;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields,
                    }];
                    r
                }
                AddCategory::SecureNote => {
                    let note_text = if *notes_stdin {
                        let mut buf = String::new();
                        std::io::stdin().read_to_string(&mut buf)?;
                        buf.trim_end_matches(['\r', '\n']).to_string()
                    } else if let Some(n) = notes {
                        n.clone()
                    } else {
                        String::new()
                    };
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: None,
                        url: None,
                        notes: None,
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut r = ItemRecord::new(it);
                    r.category = Category::SecureNote;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields: vec![gitpass_core::item::Field {
                            id: "note".into(),
                            label: "note".into(),
                            kind: FieldKind::Multiline,
                            value: Some(note_text),
                            removed: false,
                        }],
                    }];
                    r
                }
                AddCategory::CreditCard => {
                    let num = prompt_secret("GITPASS_CARD_NUMBER", "Card number: ")?;
                    if num.trim().is_empty() {
                        bail!("card number cannot be empty");
                    }
                    let cvv = prompt_secret("GITPASS_CARD_CVV", "Card CVV / verification code (empty for none): ")?;
                    let brand = gitpass_core::item::card_brand(&num);
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: None,
                        url: None,
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut fields = Vec::new();
                    if let Some(h) = holder.as_ref().filter(|h| !h.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "cardholder".into(),
                            label: "cardholder name".into(),
                            kind: FieldKind::Text,
                            value: Some(h.clone()),
                            removed: false,
                        });
                    }
                    if let Some(b) = brand {
                        fields.push(gitpass_core::item::Field {
                            id: "type".into(),
                            label: "type".into(),
                            kind: FieldKind::Text,
                            value: Some(b.to_string()),
                            removed: false,
                        });
                    }
                    fields.push(gitpass_core::item::Field {
                        id: "number".into(),
                        label: "number".into(),
                        kind: FieldKind::Concealed,
                        value: Some(num.to_string()),
                        removed: false,
                    });
                    if !cvv.trim().is_empty() {
                        fields.push(gitpass_core::item::Field {
                            id: "cvv".into(),
                            label: "verification number".into(),
                            kind: FieldKind::Concealed,
                            value: Some(cvv.to_string()),
                            removed: false,
                        });
                    }
                    if let Some(exp) = expiry.as_ref().filter(|e| !e.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "expiry".into(),
                            label: "expiry date".into(),
                            kind: FieldKind::MonthYear,
                            value: Some(exp.clone()),
                            removed: false,
                        });
                    }
                    let mut r = ItemRecord::new(it);
                    r.category = Category::CreditCard;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields,
                    }];
                    r
                }
                AddCategory::BankAccount => {
                    let acct_num = if *account_number_stdin {
                        let mut a = String::new();
                        std::io::stdin().read_line(&mut a)?;
                        let a = a.trim_end_matches(['\r', '\n']).to_string();
                        if a.trim().is_empty() { bail!("account number from stdin cannot be empty"); }
                        a
                    } else {
                        let a = prompt_secret("GITPASS_BANK_ACCOUNT_NUMBER", "Account number: ")?;
                        if a.trim().is_empty() { bail!("account number cannot be empty"); }
                        a.to_string()
                    };
                    let pin_val = if *pin_stdin {
                        let mut p = String::new();
                        std::io::stdin().read_line(&mut p)?;
                        let p = p.trim_end_matches(['\r', '\n']).to_string();
                        if p.trim().is_empty() { None } else { Some(p) }
                    } else {
                        let p = prompt_secret("GITPASS_BANK_PIN", "PIN (empty for none): ")?;
                        if p.trim().is_empty() { None } else { Some(p.to_string()) }
                    };
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: None,
                        url: None,
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut fields = Vec::new();
                    if let Some(b) = bank.as_ref().filter(|b| !b.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "bank_name".into(),
                            label: "bank name".into(),
                            kind: FieldKind::Text,
                            value: Some(b.clone()),
                            removed: false,
                        });
                    }
                    if let Some(h) = holder.as_ref().filter(|h| !h.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "account_holder".into(),
                            label: "account holder".into(),
                            kind: FieldKind::Text,
                            value: Some(h.clone()),
                            removed: false,
                        });
                    }
                    if let Some(t) = account_type.as_ref().filter(|t| !t.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "account_type".into(),
                            label: "account type".into(),
                            kind: FieldKind::Text,
                            value: Some(t.clone()),
                            removed: false,
                        });
                    }
                    fields.push(gitpass_core::item::Field {
                        id: "account_number".into(),
                        label: "account number".into(),
                        kind: FieldKind::Concealed,
                        value: Some(acct_num),
                        removed: false,
                    });
                    if let Some(r) = routing.as_ref().filter(|r| !r.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "routing_number".into(),
                            label: "routing / SWIFT / IBAN".into(),
                            kind: FieldKind::Text,
                            value: Some(r.clone()),
                            removed: false,
                        });
                    }
                    if let Some(p) = pin_val {
                        fields.push(gitpass_core::item::Field {
                            id: "pin".into(),
                            label: "PIN".into(),
                            kind: FieldKind::Concealed,
                            value: Some(p),
                            removed: false,
                        });
                    }
                    if let Some(p) = branch_phone.as_ref().filter(|p| !p.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "branch_phone".into(),
                            label: "branch phone".into(),
                            kind: FieldKind::Phone,
                            value: Some(p.clone()),
                            removed: false,
                        });
                    }
                    if let Some(a) = branch_address.as_ref().filter(|a| !a.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "branch_address".into(),
                            label: "branch address".into(),
                            kind: FieldKind::Multiline,
                            value: Some(a.clone()),
                            removed: false,
                        });
                    }
                    let mut r = ItemRecord::new(it);
                    r.category = Category::BankAccount;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields,
                    }];
                    r
                }
                AddCategory::Server => {
                    let password = if *password_stdin {
                        let mut p = String::new();
                        std::io::stdin().read_line(&mut p)?;
                        let p = p.trim_end_matches(['\r', '\n']).to_string();
                        if p.is_empty() { None } else { Some(p) }
                    } else if let Some(n) = generate {
                        let len = n.parse::<usize>().unwrap_or(24);
                        Some(generate_password(len))
                    } else {
                        let p = prompt_secret("GITPASS_ITEM_PASSWORD", "Server password (empty for none): ")?;
                        if p.is_empty() { None } else { Some(p.to_string()) }
                    };
                    let it = Item {
                        title: title.clone(),
                        username: username.clone(),
                        password: password.clone(),
                        url: url.clone(),
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut fields = Vec::new();
                    if let Some(u) = url.as_ref().filter(|u| !u.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "url".into(),
                            label: "URL or hostname".into(),
                            kind: FieldKind::Url,
                            value: Some(u.clone()),
                            removed: false,
                        });
                    }
                    if let Some(u) = username.as_ref().filter(|u| !u.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "username".into(),
                            label: "username".into(),
                            kind: FieldKind::Text,
                            value: Some(u.clone()),
                            removed: false,
                        });
                    }
                    if let Some(p) = password {
                        fields.push(gitpass_core::item::Field {
                            id: "password".into(),
                            label: "password".into(),
                            kind: FieldKind::Concealed,
                            value: Some(p),
                            removed: false,
                        });
                    }
                    if let Some(a) = admin_url.as_ref().filter(|a| !a.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "admin_console_url".into(),
                            label: "admin console URL".into(),
                            kind: FieldKind::Url,
                            value: Some(a.clone()),
                            removed: false,
                        });
                    }
                    if let Some(p) = provider.as_ref().filter(|p| !p.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "hosting_provider".into(),
                            label: "hosting provider".into(),
                            kind: FieldKind::Text,
                            value: Some(p.clone()),
                            removed: false,
                        });
                    }
                    let mut r = ItemRecord::new(it);
                    r.category = Category::Server;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields,
                    }];
                    r
                }
                AddCategory::Identity => {
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: None,
                        url: None,
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut sections = Vec::new();
                    let mut name_fields = Vec::new();
                    if let Some(f) = first_name.as_ref().filter(|v| !v.is_empty()) {
                        name_fields.push(gitpass_core::item::Field {
                            id: "first".into(),
                            label: "first name".into(),
                            kind: FieldKind::Text,
                            value: Some(f.clone()),
                            removed: false,
                        });
                    }
                    if let Some(l) = last_name.as_ref().filter(|v| !v.is_empty()) {
                        name_fields.push(gitpass_core::item::Field {
                            id: "last".into(),
                            label: "last name".into(),
                            kind: FieldKind::Text,
                            value: Some(l.clone()),
                            removed: false,
                        });
                    }
                    if !name_fields.is_empty() {
                        sections.push(Section {
                            id: "name".into(),
                            title: Some("Identification".into()),
                            fields: name_fields,
                        });
                    }

                    if let Some(a) = address.as_ref().filter(|v| !v.is_empty()) {
                        sections.push(Section {
                            id: "address".into(),
                            title: Some("Address".into()),
                            fields: vec![gitpass_core::item::Field {
                                id: "address".into(),
                                label: "address".into(),
                                kind: FieldKind::Multiline,
                                value: Some(a.clone()),
                                removed: false,
                            }],
                        });
                    }

                    let mut contact_fields = Vec::new();
                    if let Some(e) = email.as_ref().filter(|v| !v.is_empty()) {
                        contact_fields.push(gitpass_core::item::Field {
                            id: "email".into(),
                            label: "email".into(),
                            kind: FieldKind::Email,
                            value: Some(e.clone()),
                            removed: false,
                        });
                    }
                    if let Some(p) = phone.as_ref().filter(|v| !v.is_empty()) {
                        contact_fields.push(gitpass_core::item::Field {
                            id: "phone".into(),
                            label: "phone".into(),
                            kind: FieldKind::Phone,
                            value: Some(p.clone()),
                            removed: false,
                        });
                    }
                    if !contact_fields.is_empty() {
                        sections.push(Section {
                            id: "contact".into(),
                            title: Some("Contact".into()),
                            fields: contact_fields,
                        });
                    }

                    let mut r = ItemRecord::new(it);
                    r.category = Category::Identity;
                    r.sections = sections;
                    r
                }
                AddCategory::Wifi => {
                    let password = if *password_stdin {
                        let mut p = String::new();
                        std::io::stdin().read_line(&mut p)?;
                        let p = p.trim_end_matches(['\r', '\n']).to_string();
                        if p.is_empty() { None } else { Some(p) }
                    } else {
                        let p = if std::env::var("GITPASS_WIFI_PASSWORD").is_ok() {
                            prompt_secret("GITPASS_WIFI_PASSWORD", "Wi-Fi password (empty for none): ")?
                        } else {
                            prompt_secret("GITPASS_ITEM_PASSWORD", "Wi-Fi password (empty for none): ")?
                        };
                        if p.is_empty() { None } else { Some(p.to_string()) }
                    };
                    let it = Item {
                        title: title.clone(),
                        username: None,
                        password: password.clone(),
                        url: None,
                        notes: notes.clone(),
                        totp: None,
                        updated_at: store::now(),
                    };
                    let mut fields = Vec::new();
                    if let Some(s) = ssid.as_ref().filter(|s| !s.is_empty()) {
                        fields.push(gitpass_core::item::Field {
                            id: "ssid".into(),
                            label: "network name".into(),
                            kind: FieldKind::Text,
                            value: Some(s.clone()),
                            removed: false,
                        });
                    }
                    if let Some(p) = password {
                        fields.push(gitpass_core::item::Field {
                            id: "password".into(),
                            label: "wireless password".into(),
                            kind: FieldKind::Concealed,
                            value: Some(p),
                            removed: false,
                        });
                    }
                    if let Some(s) = security.as_ref().filter(|s| !s.is_empty()) {
                        let norm = match s.trim().to_ascii_lowercase().as_str() {
                            "wpa2" => "WPA2".to_string(),
                            "wpa3" => "WPA3".to_string(),
                            "wep" => "WEP".to_string(),
                            "none" => "none".to_string(),
                            _ => s.trim().to_string(),
                        };
                        fields.push(gitpass_core::item::Field {
                            id: "security".into(),
                            label: "security".into(),
                            kind: FieldKind::Text,
                            value: Some(norm),
                            removed: false,
                        });
                    }
                    let mut r = ItemRecord::new(it);
                    r.category = Category::Wifi;
                    r.sections = vec![Section {
                        id: "main".into(),
                        title: None,
                        fields,
                    }];
                    r
                }
                AddCategory::SshKey => {
                    use gitpass_core::ssh::SshKey;
                    use zeroize::Zeroizing;
                    let k = if let Some(file) = private_key_file {
                        let pem = Zeroizing::new(std::fs::read_to_string(file).with_context(|| format!("read key file {}", file.display()))?);
                        match SshKey::import_openssh(&pem, None) {
                            Ok(k) => k,
                            Err(e) if e.to_string().contains("encrypted") => {
                                let p = prompt_secret("GITPASS_SSH_PASSPHRASE", "Key passphrase: ")?;
                                SshKey::import_openssh(&pem, Some(&p))?
                            }
                            Err(e) => return Err(e),
                        }
                    } else {
                        SshKey::generate_ed25519(title)?
                    };
                    let mut r = k.to_record(title)?;
                    if let Some(n) = notes {
                        r.item.notes = Some(n.clone());
                    }
                    r
                }
            };
            println!("added {}", s.add_record(&vid, rec)?);
        }
        Cmd::Get { vault, query, password_only, pick, copy, field, clear_secs, quiet, json, reveal, include_archived } => {
            if *json && *password_only {
                bail!("cannot combine --json and --password-only");
            }
            if *json && *copy {
                bail!("cannot combine --json and --copy");
            }
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let (id, rec) = pick_record(&s, &vid, query, *pick, *include_archived)?;
            if *json {
                if *reveal {
                    let _ = s.touch_item_used(&id);
                }
                let last_used_at = s.last_used(&id).map(|t| gitpass_core::device::iso8601(t as i64));
                let detail = item_detail(&id, &rec, *reveal, last_used_at);
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else if *copy {
                let _ = s.touch_item_used(&id);
                do_copy(&field_value(&rec.item, *field)?, *clear_secs, *quiet, "field", field_markers(*field))?;
            } else if *password_only {
                let _ = s.touch_item_used(&id);
                let secret = rec.item.password.clone().or_else(|| {
                    rec.view_sections()
                        .into_iter()
                        .flat_map(|s| s.fields)
                        .find(|f| f.kind.is_secret() && f.value.is_some())
                        .and_then(|f| f.value)
                }).unwrap_or_default();
                println!("{secret}");
            } else {
                print_item(&id, &rec, *reveal);
            }
        }
        Cmd::Copy { vault, query, field, pick, clear_secs, quiet } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let (id, it) = pick_item(&s, &vid, query, *pick)?;
            let _ = s.touch_item_used(&id);
            do_copy(&field_value(&it, *field)?, *clear_secs, *quiet, &format!("'{}'", it.title), field_markers(*field))?;
        }
        Cmd::Totp { vault, query, pick, copy, clear_secs } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let (id, it) = pick_item(&s, &vid, query, *pick)?;
            let t = it.totp.as_deref().ok_or_else(|| anyhow::anyhow!("item '{}' has no TOTP secret", it.title))?;
            let (code, rem) = gitpass_core::totp::code_now(&gitpass_core::totp::parse(t)?);
            println!("{code}  ({rem}s remaining)");
            if *copy {
                let _ = s.touch_item_used(&id);
                do_copy(&code, *clear_secs, true, "TOTP code", Some(PasteboardMarkers::SECRET))?;
            }
        }
        Cmd::Find { vault, query, json, include_archived } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let mut recs = s.list_records(&vid)?;
            if !include_archived {
                recs.retain(|(_, _, r)| r.deleted_at.is_none() && !r.archived);
            }
            let vname = s.vault_meta(&vid).map(|m| m.name.clone()).unwrap_or_default();
            let usage = s.all_last_used();
            let ranked = gitpass_core::search::rank_by_candidate(
                query,
                recs.into_iter().map(|(id, _, rec)| (id, rec)),
                |r| gitpass_core::search::CandidateInfo::from_record(r, Some(vname.as_str())),
            );
            if *json {
                let summaries: Vec<CliItemSummary> = ranked.iter().map(|(_, id, rec)| {
                    let mut sum = item_summary(id, rec);
                    sum.last_used = usage.get(id).copied();
                    sum
                }).collect();
                println!("{}", serde_json::to_string_pretty(&summaries)?);
            } else {
                for (sc, id, rec) in ranked {
                    let col = if rec.category == Category::ApiCredential {
                        gitpass_core::mask::api_credential_secret(&rec)
                            .map(gitpass_core::mask::secret_hint)
                            .or_else(|| rec.item.username.clone())
                            .unwrap_or_default()
                    } else {
                        rec.item.username.clone().unwrap_or_default()
                    };
                    println!("{id}  {}  {}  {}  (score {sc})", rec.item.title, col, rec.item.url.clone().unwrap_or_default());
                }
            }
        }
        Cmd::List { vault, query, category, sort, json, include_archived } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let mut recs = s.list_records(&vid)?;
            if !include_archived {
                recs.retain(|(_, _, r)| r.deleted_at.is_none() && !r.archived);
            }
            if let Some(cat) = category {
                let want = Category::from(*cat);
                recs.retain(|(_, _, r)| r.category == want);
            }
            let usage = s.all_last_used();
            let items: Vec<(String, ItemRecord)> = match query {
                Some(q) => {
                    let vname = s.vault_meta(&vid).map(|m| m.name.clone()).unwrap_or_default();
                    gitpass_core::search::rank_by_candidate(
                        q,
                        recs.into_iter().map(|(id, _, rec)| (id, rec)),
                        |r| gitpass_core::search::CandidateInfo::from_record(r, Some(vname.as_str())),
                    )
                    .into_iter()
                    .map(|(_, id, rec)| (id, rec))
                    .collect()
                }
                None => {
                    if *sort == Some(ListSort::Recent) {
                        recs.sort_by(|a, b| {
                            let a_u = usage.get(&a.0).copied().unwrap_or(0);
                            let b_u = usage.get(&b.0).copied().unwrap_or(0);
                            b_u.cmp(&a_u).then_with(|| a.2.item.title.to_lowercase().cmp(&b.2.item.title.to_lowercase()))
                        });
                    } else {
                        recs.sort_by(|a, b| a.2.item.title.to_lowercase().cmp(&b.2.item.title.to_lowercase()));
                    }
                    recs.into_iter().map(|(id, _, rec)| (id, rec)).collect()
                }
            };
            if *json {
                let summaries: Vec<CliItemSummary> = items.iter().map(|(id, rec)| {
                    let mut sum = item_summary(id, rec);
                    sum.last_used = usage.get(id).copied();
                    sum
                }).collect();
                println!("{}", serde_json::to_string_pretty(&summaries)?);
            } else {
                for (id, rec) in items {
                    let col = if rec.category == Category::ApiCredential {
                        gitpass_core::mask::api_credential_secret(&rec)
                            .map(gitpass_core::mask::secret_hint)
                            .or_else(|| rec.item.username.clone())
                            .unwrap_or_default()
                    } else {
                        rec.item.username.clone().unwrap_or_default()
                    };
                    println!("{id}  {}  {col}", rec.item.title);
                }
            }
        }
        Cmd::Edit {
            item,
            item2,
            vault,
            title,
            username,
            url,
            notes,
            totp,
            password,
            generate,
            tag,
            untag,
            favorite,
            unfavorite,
            pick,
            json,
        } => {
            let s = unlock(&cli, &root)?;
            let (vid, iid, mut rec) = resolve_target_item(&s, vault.as_deref(), item, item2.as_deref(), *pick, false)?;
            if let Some(v) = title { rec.item.title = v.clone(); }
            if let Some(v) = username { rec.item.username = Some(v.clone()); }
            if let Some(v) = url { rec.item.url = Some(v.clone()); }
            if let Some(v) = notes { rec.item.notes = Some(v.clone()); }
            if let Some(v) = totp { rec.item.totp = Some(v.clone()); }
            if let Some(n) = generate { rec.item.password = Some(generate_password(*n)); }
            else if *password { rec.item.password = Some(prompt_secret("GITPASS_ITEM_PASSWORD", "New item password: ")?.to_string()); }

            for u in untag {
                let u_trim = u.trim();
                rec.tags.retain(|t| t != u_trim);
            }
            for t in tag {
                let t_trim = t.trim();
                if !t_trim.is_empty() && !rec.tags.iter().any(|existing| existing == t_trim) {
                    rec.tags.push(t_trim.to_string());
                }
            }
            if *favorite {
                rec.favorite = true;
            }
            if *unfavorite {
                rec.favorite = false;
            }
            rec.item.updated_at = store::now();
            s.update_record(&vid, &iid, rec)?;

            if *json {
                let (_, updated) = s.read_record(&vid, &iid)?;
                let detail = item_detail(&iid, &updated, false, None);
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else {
                println!("updated {iid}");
            }
        }
        Cmd::Archive { item, item2, vault, pick, json } => {
            let s = unlock(&cli, &root)?;
            let (vid, iid, _rec) = resolve_target_item(&s, vault.as_deref(), item, item2.as_deref(), *pick, false)?;
            s.set_archived(&vid, &iid, true)?;
            if *json {
                let (_, updated) = s.read_record(&vid, &iid)?;
                let detail = item_detail(&iid, &updated, false, None);
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else {
                println!("archived {iid}");
            }
        }
        Cmd::Unarchive { item, item2, vault, pick, json } => {
            let s = unlock(&cli, &root)?;
            let (vid, iid, rec) = resolve_target_item(&s, vault.as_deref(), item, item2.as_deref(), *pick, true)?;
            if !rec.archived {
                bail!("item '{}' is not archived", rec.item.title);
            }
            s.set_archived(&vid, &iid, false)?;
            if *json {
                let (_, updated) = s.read_record(&vid, &iid)?;
                let detail = item_detail(&iid, &updated, false, None);
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else {
                println!("unarchived {iid}");
            }
        }
        Cmd::Undelete { item, item2, vault, pick, json } => {
            let s = unlock(&cli, &root)?;
            let (vid, iid, _rec) = resolve_deleted_item(&s, vault.as_deref(), item, item2.as_deref(), *pick)?;
            s.restore(&vid, &iid)?;
            if *json {
                let (_, updated) = s.read_record(&vid, &iid)?;
                let detail = item_detail(&iid, &updated, false, None);
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else {
                println!("restored {iid}");
            }
        }
        Cmd::Mv { item, item2, vault, to, pick, json } => {
            let s = unlock(&cli, &root)?;
            let (from_vid, iid, _rec) = resolve_target_item(&s, vault.as_deref(), item, item2.as_deref(), *pick, false)?;
            let to_vid = s.resolve_vault(to)?;
            if from_vid == to_vid {
                bail!("item is already in vault '{to}'");
            }
            let new_id = s.move_item(&from_vid, &iid, &to_vid)?;
            if *json {
                let (_, updated) = s.read_record(&to_vid, &new_id)?;
                let detail = item_detail(&new_id, &updated, false, None);
                println!("{}", serde_json::to_string_pretty(&detail)?);
            } else {
                println!("moved {iid} to {to} (now {new_id})");
            }
        }
        Cmd::Rm { vault, id } => {
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let iid = s.resolve_item(&vid, id)?;
            s.remove_item(&vid, &iid)?;
            println!("removed {iid}");
        }
        Cmd::History { item, vault, json } => {
            let s = unlock(&cli, &root)?;
            let (vid, iid) = resolve_item_across_vaults(&s, vault.as_deref(), item)?;
            let (cur_v, r) = s.read_record(&vid, &iid)?;
            let mut entries = Vec::new();
            entries.push(CliHistoryEntry {
                version: cur_v,
                updated_at: ts(&r.item.updated_at),
                current: true,
            });
            for h in r.history.iter().rev() {
                entries.push(CliHistoryEntry {
                    version: h.version,
                    updated_at: ts(&h.updated_at),
                    current: false,
                });
            }
            if *json {
                println!("{}", serde_json::to_string_pretty(&entries)?);
            } else {
                for e in &entries {
                    if e.current {
                        println!("version {} (current)  {}", e.version, e.updated_at);
                    } else {
                        println!("version {}            {}", e.version, e.updated_at);
                    }
                }
            }
        }
        Cmd::Restore { item, version, vault } => {
            let s = unlock(&cli, &root)?;
            let (vid, iid) = resolve_item_across_vaults(&s, vault.as_deref(), item)?;
            let new_v = s.restore_version(&vid, &iid, *version)?;
            println!("restored {iid} to version {version} (now version {new_v})");
        }
        Cmd::Gen { command, no_history, len, words, pin, separator, capitalize, no_symbols, no_digits, copy, clear_secs } => {
            if let Some(cmd) = command {
                match cmd {
                    GenSubcommand::History { action, json, reveal } => {
                        let s = unlock(&cli, &root)?;
                        match action {
                            Some(GenHistoryAction::Clear) => {
                                s.clear_generator_history()?;
                                eprintln!("cleared generator history");
                            }
                            None => {
                                let history = s.read_generator_history()?;
                                if *json {
                                    let dtos: Vec<serde_json::Value> = history.entries.iter().map(|e| {
                                        serde_json::json!({
                                            "id": e.id,
                                            "value": if *reveal { e.value.as_str() } else { "••••••••" },
                                            "created_at": e.created_at,
                                            "source": e.source,
                                            "host": e.host,
                                        })
                                    }).collect();
                                    println!("{}", serde_json::to_string_pretty(&dtos)?);
                                } else if history.entries.is_empty() {
                                    println!("(no generated passwords in history)");
                                } else {
                                    for entry in &history.entries {
                                        let ts = gitpass_core::device::iso8601(entry.created_at as i64);
                                        let val = if *reveal { entry.value.as_str() } else { "••••••••" };
                                        let host_part = entry.host.as_deref().map(|h| format!("  ({h})")).unwrap_or_default();
                                        println!("{ts}  [{}]  {val}{host_part}", entry.source);
                                    }
                                }
                            }
                        }
                        return Ok(());
                    }
                }
            }
            let args = gitpass_cli::gen::Args { len: *len, words: *words, pin: *pin, separator: *separator,
                capitalize: *capitalize, no_symbols: *no_symbols, no_digits: *no_digits, copy: *copy, clear_secs: *clear_secs };
            let mut out = std::io::stdout();
            let pw = gitpass_cli::gen::generate_pw(&args)?;
            if !*no_history && root.join("format.json").exists() {
                if let Ok(s) = unlock(&cli, &root) {
                    let _ = s.record_generated_password(&pw, "cli", None);
                }
            }
            gitpass_cli::gen::output_pw(&args, &pw, gitpass_cli::clip::system, &mut out, std::thread::sleep)?;
        }
        Cmd::Run { env_file, no_masking, cmd } => {
            use gitpass_cli::inject;

            if env_file.is_empty() && !std::env::vars().any(|(_, v)| v.contains("gp://")) {
                bail!("no gp:// references found: no --env-file was given and no inherited environment variables contain gp:// references (usage: gitpass run [--env-file F] -- <cmd>)");
            }

            let mut file_texts = Vec::with_capacity(env_file.len());
            for f in env_file {
                let text = Zeroizing::new(std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?);
                file_texts.push(text);
            }
            let file_slices: Vec<&str> = file_texts.iter().map(|s| s.as_str()).collect();

            let s = unlock(&cli, &root)?;
            let sr = SessionResolver(&s);
            let mut res = inject::Resolved::new(&sr);
            let env = inject::resolve_run_env(std::env::vars(), &file_slices, &mut res)?;
            drop(file_texts);
            let secrets = std::mem::take(&mut res.secrets);
            drop(s);
            let code = inject::run_child(cmd, &env, &secrets, !*no_masking)?;
            drop(env); drop(secrets);
            std::process::exit(code);
        }
        Cmd::Inject { input, output } => {
            use gitpass_cli::inject;
            use std::io::Read;
            let text = Zeroizing::new(match input {
                Some(p) => std::fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?,
                None => { let mut b = String::new(); std::io::stdin().read_to_string(&mut b)?; b }
            });
            let s = unlock(&cli, &root)?;
            let sr = SessionResolver(&s);
            let out = inject::render_template(&text, &mut inject::Resolved::new(&sr))?;
            match output {
                Some(p) => {
                    gitpass_core::fsutil::atomic_write(p, out.as_bytes())?;
                    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))?; }
                    eprintln!("wrote {} (mode 0600; contains plaintext secrets)", p.display());
                }
                None => { use std::io::Write; std::io::stdout().write_all(out.as_bytes())?; }
            }
        }
        Cmd::Ref { vault, query, field, pick, copy } => {
            use gitpass_cli::inject::{field_segment, Resolver, SecretRef};
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let (id, rec) = pick_record(&s, &vid, query, *pick, false)?;
            let vaults = s.list_vaults()?;
            let vname = vaults.iter().find(|(v, _)| *v == vid).map(|(_, m)| m.name.clone()).unwrap_or_default();
            // Name the vault and item by title only while that name resolves to exactly one of them.
            let vault_seg = if !vname.is_empty() && vaults.iter().filter(|(_, m)| m.name == vname).count() == 1 { vname } else { vid.clone() };
            let same_title = s.list_items(&vid)?.iter().filter(|(_, it)| it.title.eq_ignore_ascii_case(&rec.item.title)).count();
            let item_seg = if same_title == 1 && !rec.item.title.is_empty() { rec.item.title.clone() } else { id.clone() };
            let fields: Vec<(String, String)> = rec.sections.iter().flat_map(|sec| sec.fields.iter())
                .filter(|f| !f.removed).map(|f| (f.id.clone(), f.label.clone())).collect();
            let field_seg = field_segment(field.as_deref().unwrap_or("password"), &fields)?;
            let r = SecretRef { vault: vault_seg, item: item_seg, field: field_seg };
            // Round trip through the same resolver `run`/`inject` use, so a printed reference always works.
            SessionResolver(&s).resolve(&r)?;
            let text = r.to_string();
            if *copy { do_copy(&text, 0, false, "secret reference", None)?; } else { println!("{text}"); }
        }
        Cmd::Remote { cmd } => match cmd {
            RemoteCmd::Add { url, allow_local } => {
                print!("{}", gitpass_core::remote::add(&root, url, *allow_local)?);
                println!("connected: origin = {}", url.trim());
            }
            RemoteCmd::Show => {
                let i = gitpass_core::remote::show(&root)?;
                match &i.url {
                    Some(u) => println!("origin: {u}\nbranch: {}\nahead: {}  behind: {}", i.branch.as_deref().unwrap_or("?"), i.ahead, i.behind),
                    None => println!("not connected (run: gitpass remote add git@github.com:YOU/REPO.git)"),
                }
            }
            RemoteCmd::Remove => { gitpass_core::remote::remove(&root)?; println!("disconnected: removed 'origin' (local files and history kept)"); }
        },
        Cmd::Sync { merge } => {
            let (_s, pw, sk) = unlock_full(&cli, &root)?;
            if *merge {
                drop(_s);
                let opts = UnlockOpts { accept_rollback: cli.accept_rollback, target_kdf: gitpass_core::store::read_format(&root)?.kdf };
                let r = gitpass_core::sync::sync_merge(&root, &pw, &sk, &opts)?;
                print!("{}", r.log);
                for (vid, iid, m) in &r.merged {
                    println!("merged {vid}/{iid}: remote={:?} local={:?}", m.from_remote, m.from_local);
                    if m.has_conflicts() { println!("  CONFLICT (equal clocks, kept local): {:?}", m.conflicts); }
                }
                for (vid, iid) in &r.reapplied { println!("re-applied local-only item {vid}/{iid}"); }
            } else {
                print!("{}", gitpass_core::sync::sync(&root)?);
            }
            // verify-only: no device/KDF writes after the push (keeps the working tree clean)
            let s = Session::reopen_verify(&root, &pw, &sk, cli.accept_rollback)
                .context("post-pull integrity check failed; the pre-pull backup is in .gitpass-backup-*/")?;
            println!("post-pull integrity verified (manifest epoch {})", s.epoch()?);
        }
        Cmd::Passwd => {
            let (mut s, _pw, sk) = unlock_full(&cli, &root)?;
            let npw = new_password()?;
            let k = s.fmt.kdf.max_with(&KdfParams::default());
            s.change_password(&npw, &sk, k)?;
            println!("master password changed (manifest epoch {})", s.epoch()?);
        }
        Cmd::RotateSecretKey { save_secret_key } => {
            let (mut s, pw, _sk) = unlock_full(&cli, &root)?;
            let k = s.fmt.kdf.max_with(&KdfParams::default());
            let nsk = s.rotate_secret_key(&pw, k)?;
            let enc = Zeroizing::new(nsk.encode());
            if *save_secret_key || root.join("secret.key").exists() { save_sk(&root, &enc)?; }
            print_kit(&root, &enc);
            println!("Secret Key rotated. Your OLD Secret Key no longer opens the current keyset.");
        }
        Cmd::RotateAccountKey { with_vault_keys } => {
            let (mut s, pw, sk) = unlock_full(&cli, &root)?;
            let b = s.rotate_account_key(&pw, &sk)?;
            println!("account key rotated (backup of previous state: {})", b.display());
            if *with_vault_keys {
                for (vid, m) in s.list_vaults()? { s.rotate_vault(&vid)?; println!("rotated vault key for {}", m.name); }
            }
        }
        Cmd::Device { cmd } => {
            let (mut s, pw, sk) = unlock_full(&cli, &root)?;
            match cmd {
                DeviceCmd::List => {
                    let reg = s.devices()?;
                    println!("{:<36}  {:<20}  {:<20}  STATUS", "ID", "NAME", "LAST SEEN (UTC)");
                    for d in &reg.devices {
                        let mut st = match &d.revoked_at { Some(t) if d.revoked => format!("REVOKED {t}"), _ if d.revoked => "REVOKED".into(), _ => "active".into() };
                        if d.id == s.device_id { st.push_str(" (this device)"); }
                        println!("{:<36}  {:<20}  {:<20}  {st}", d.id, d.name, d.last_seen_at);
                    }
                }
                DeviceCmd::Rename { id, name } => { let id = s.rename_device(id, name)?; println!("renamed {id} -> {name}"); }
                DeviceCmd::Revoke { id, marker_only, rotate_credentials, save_secret_key } => {
                    let reg = s.devices()?;
                    let full = reg.resolve(id)?;
                    let d = reg.get(&full).unwrap();
                    let this = if full == s.device_id { " (THIS device)" } else { "" };
                    if !cli.yes && !confirm(&format!("Revoke device {} '{}'{this}?", d.id, d.name))? { bail!("aborted"); }
                    if *marker_only {
                        s.revoke_device(&full)?;
                        println!("revoked {full} (marker only; no keys rotated)");
                    } else {
                        let rep = s.revoke_device_and_rotate(&full, &pw, &sk)?;
                        println!("revoked {full}; rotated account key and {} vault key(s) (backup: {})", rep.vaults_rotated.len(), rep.account_key_backup.display());
                        eprintln!("NOTE: the revoked device cannot decrypt changes made from now on, but it KEEPS anything it already");
                        eprintln!("copied (old repo clone, items, old keys). Push now so other devices pick up the new keys.");
                    }
                    if *rotate_credentials {
                        let npw = new_password()?;
                        let k = s.fmt.kdf.max_with(&KdfParams::default());
                        let nsk = s.rotate_credentials(&npw, k)?;
                        let enc = Zeroizing::new(nsk.encode());
                        if *save_secret_key || root.join("secret.key").exists() { save_sk(&root, &enc)?; }
                        print_kit(&root, &enc);
                        println!("master password and Secret Key rotated.");
                    } else {
                        eprintln!("If the revoked device knew your master password + Secret Key it can still derive the new keys:");
                        eprintln!("rerun with --rotate-credentials, or run `gitpass passwd` and `gitpass rotate-secret-key`.");
                    }
                }
                DeviceCmd::Unrevoke { id } => { let id = s.unrevoke_device(id)?; println!("unrevoked {id}"); }
            }
        }
        Cmd::Import { vault, from, file, dry_run, allow_duplicates } => {
            use gitpass_core::import;
            let resolved = import::resolve_import_path(file);
            let data = Zeroizing::new(std::fs::read(&resolved).with_context(|| format!("reading {}", resolved.display()))?);
            let fmt: import::ImportFormat = match from.as_deref() {
                Some(f) if !f.is_empty() => f.parse()?,
                _ => import::detect_format_bytes(&data)
                    .ok_or_else(|| anyhow::anyhow!("could not auto-detect format; specify --from <format>"))?,
            };
            let pass = if fmt == import::ImportFormat::GitpassJson && import::is_encrypted_export(&String::from_utf8_lossy(&data)) {
                Some(prompt_secret("GITPASS_EXPORT_PASSPHRASE", "Export passphrase: ")?) } else { None };
            let parsed = import::parse_bytes(fmt, &data, pass.as_deref().map(|s| s.as_str()))?;
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let r = import::import_items(&s, &vid, parsed, import::ImportOpts { dry_run: *dry_run, allow_duplicates: *allow_duplicates })?;
            for w in &r.warnings { eprintln!("warning: {w}"); }
            for d in &r.duplicates { eprintln!("duplicate skipped: {d}"); }
            println!("{}parsed {}, {} {}, {} duplicate(s) skipped", if *dry_run { "[dry run] " } else { "" },
                r.parsed, if *dry_run { "would add" } else { "added" }, r.added, r.duplicates.len());
        }
        Cmd::Export { vault, format, out, encrypted } => {
            use gitpass_core::import;
            let out = &std::path::absolute(out)?;
            if *encrypted && *format == ExportFmt::Csv { bail!("--encrypted only supports --format gitpass-json"); }
            let s = unlock(&cli, &root)?;
            let vid = s.resolve_vault(vault)?;
            let recs: Vec<_> = s.list_records(&vid)?.into_iter().filter(|(_, _, r)| r.deleted_at.is_none()).map(|(_, _, r)| r).collect();
            let items: Vec<Item> = recs.iter().map(|r| r.item.clone()).collect();
            let body = if *encrypted {
                let p = prompt_secret("GITPASS_EXPORT_PASSPHRASE", "Export passphrase: ")?;
                if std::env::var("GITPASS_EXPORT_PASSPHRASE").is_err() {
                    let p2 = prompt_secret("__none__", "Confirm export passphrase: ")?;
                    if *p != *p2 { bail!("passphrases do not match"); }
                }
                Zeroizing::new(import::export_records_encrypted(&recs, &p, &import::export_kdf_default())?)
            } else {
                eprintln!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                eprintln!("!! WARNING: PLAINTEXT EXPORT. {} will contain ALL passwords,", out.display());
                eprintln!("!! TOTP secrets and notes UNENCRYPTED. Anyone who reads the file owns");
                eprintln!("!! these accounts. Prefer --encrypted. Delete the file when done.");
                eprintln!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
                if !cli.yes && !confirm("Write plaintext export?")? { bail!("aborted (pass --yes to confirm)"); }
                Zeroizing::new(match format { ExportFmt::GitpassJson => import::export_records_json(&recs)?, ExportFmt::Csv => import::export_csv(&items)? })
            };
            gitpass_core::fsutil::atomic_write(out, body.as_bytes())?;
            #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o600))?; }
            println!("exported {} item(s) to {}{}", items.len(), out.display(), if *encrypted { " (encrypted)" } else { " (PLAINTEXT)" });
        }
        Cmd::Audit { vault, hibp, max_age_days, expiring_days, unused_months, json, include_ignored } => {
            use gitpass_core::{audit, passkey, Category};
            let s = unlock(&cli, &root)?;
            let vids: Vec<(String, String)> = match vault {
                Some(v) => { let id = s.resolve_vault(v)?; vec![(id.clone(), s.vault_meta(&id)?.name.clone())] }
                None => s.list_vaults()?.into_iter().map(|(id, m)| (id, m.name.clone())).collect(),
            };
            let mut all = vec![];
            let mut passkeys = std::collections::BTreeSet::new();
            for (vid, name) in &vids {
                let mut rows = vec![];
                for (iid, _, rec) in s.list_records(vid)? {
                    if rec.deleted_at.is_some() || rec.archived { continue; }
                    if rec.category == Category::Passkey {
                        if let Some(v) = rec.find_field(passkey::F_RP_ID).and_then(|f| f.value) { passkeys.insert(v); }
                    } else {
                        rows.push((name.clone(), iid, rec));
                    }
                }
                rows.sort_by(|a, b| a.2.item.title.to_lowercase().cmp(&b.2.item.title.to_lowercase()));
                all.extend(rows);
            }
            let client = UreqHibp(ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(15)).build());
            let opts = audit::AuditOpts { max_age_days: *max_age_days, expiring_days: *expiring_days, include_ignored: *include_ignored,
                unused_months: (*unused_months > 0).then_some(*unused_months), last_used: s.all_last_used(), ..Default::default() };
            let r = audit::audit_with_passkeys(&all, &passkeys, &opts, if *hibp { Some(&client) } else { None })?;
            if *json {
                println!("{}", serde_json::to_string_pretty(&r)?);
                return Ok(());
            }
            println!("audited {} item(s) in {} vault(s){}", r.items_checked, vids.len(), if *hibp { " (with HIBP)" } else { "" });
            let unused_label = if *unused_months > 0 { format!(">{unused_months}mo") } else { "off".to_string() };
            println!("  weak: {}  reused: {} ({} group(s))  duplicates: {}  old (>{max_age_days}d): {}  expired: {}  expiring soon (<={expiring_days}d): {}  insecure URLs: {}  missing TOTP: {}  2FA available: {}  passkeys available: {}{}  weak SSH keys: {}  unused ({unused_label}): {}  ignored: {}",
                r.weak, r.reused, r.reused_groups, r.duplicates, r.old, r.expired, r.expiring_soon, r.insecure_urls, r.missing_totp, r.totp_available, r.passkey_available,
                if *hibp { format!("  pwned: {}", r.pwned) } else { String::new() },
                r.weak_ssh_keys,
                r.unused,
                r.ignored);
            for f in &r.per_item {
                println!("\n[{}] {} ({})", f.vault, f.title, &f.id[..f.id.len().min(8)]);
                for x in &f.findings { println!("  - {x}"); }
            }
            if *include_ignored && !r.ignored_per_item.is_empty() {
                println!("\nignored findings ({}):", r.ignored);
                for f in &r.ignored_per_item {
                    println!("\n[{}] {} ({}) [ignored]", f.vault, f.title, &f.id[..f.id.len().min(8)]);
                    for x in &f.findings { println!("  - {x} (ignored)"); }
                }
            }
        }
        Cmd::Migrate => {
            let pw = prompt_secret("GITPASS_PASSWORD", "Master password: ")?;
            let sk = load_secret_key(&root)?;
            do_migrate(&root, &pw, &sk)?;
        }
        Cmd::Completions { .. } => unreachable!(),
    }
    Ok(())
}

/// Default agent socket (same path the desktop app uses).
fn default_ssh_socket() -> PathBuf {
    gitpass_core::ssh::default_ssh_agent_socket_path()
}

fn ssh_cmd(cli: &Cli, root: &Path, cmd: &SshCmd) -> Result<()> {
    use gitpass_core::ssh::{self, SshKey};
    match cmd {
        SshCmd::Config { socket } => println!("{}", ssh::identity_agent_line(&socket.clone().unwrap_or_else(default_ssh_socket))),
        SshCmd::Generate { vault, title, comment } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let k = SshKey::generate_ed25519(comment.as_deref().unwrap_or(title))?;
            let iid = s.add_record(&vid, k.to_record(title)?)?;
            println!("id:          {iid}\npublic key:  {}\nfingerprint: {}", k.info.public_key, k.info.fingerprint);
        }
        SshCmd::Import { vault, file, title } => {
            let pem = Zeroizing::new(std::fs::read_to_string(file).with_context(|| format!("read {}", file.display()))?);
            let k = match SshKey::import_openssh(&pem, None) {
                Ok(k) => k,
                Err(e) if e.to_string().contains("encrypted") => {
                    let p = prompt_secret("GITPASS_SSH_PASSPHRASE", "Key passphrase: ")?;
                    SshKey::import_openssh(&pem, Some(&p))?
                }
                Err(e) => return Err(e),
            };
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let t = title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| if k.info.comment.is_empty() { file.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default() } else { k.info.comment.clone() });
            let iid = s.add_record(&vid, k.to_record(&t)?)?;
            println!("id:          {iid}\npublic key:  {}\nfingerprint: {}", k.info.public_key, k.info.fingerprint);
        }
        SshCmd::List { vault } => {
            let s = unlock(cli, root)?;
            let only = match vault { Some(v) => Some(s.resolve_vault(v)?), None => None };
            for i in ssh::session_identities(&s)? {
                if only.as_ref().is_some_and(|v| *v != i.vault_id) { continue; }
                println!("{}  {}  {}  {}", i.item_id, i.key.info.key_type, i.key.info.fingerprint, i.title);
            }
        }
        SshCmd::Public { vault, query } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let iid = s.resolve_item(&vid, query)?;
            let (_, r) = s.read_record(&vid, &iid)?;
            let k = SshKey::from_record(&r)?;
            println!("{}", k.info.public_key);
        }
        SshCmd::Agent { socket } => {
            #[cfg(not(unix))]
            { let _ = socket; bail!("`gitpass ssh agent` is not supported on Windows yet"); }
            #[cfg(unix)]
            {
            use std::os::unix::fs::PermissionsExt;
            use std::os::unix::net::{UnixListener, UnixStream};
            let sp = socket.clone().unwrap_or_else(default_ssh_socket);
            let s = unlock(cli, root)?;
            let ids = ssh::session_identities(&s)?;
            drop(s);
            if let Some(d) = sp.parent() { std::fs::create_dir_all(d)?; std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700))?; }
            if sp.exists() {
                if UnixStream::connect(&sp).is_ok() { bail!("an agent is already listening on {}", sp.display()); }
                std::fs::remove_file(&sp)?;
            }
            let l = UnixListener::bind(&sp)?;
            std::fs::set_permissions(&sp, std::fs::Permissions::from_mode(0o600))?;
            eprintln!("gitpass ssh agent: {} key(s) on {}", ids.len(), sp.display());
            eprintln!("export SSH_AUTH_SOCK=\"{}\"", sp.display());
            let keys = std::sync::Arc::new(ssh::StaticKeys { ids, approve: Box::new(|_, _| true) });
            for c in l.incoming() {
                let Ok(mut c) = c else { continue };
                let k = keys.clone();
                std::thread::spawn(move || { let _ = ssh::serve_stream(&mut c, &*k); });
            }
            }
        }
        SshCmd::GitSigning { vault, query, socket, global: _, local, print: _, apply, yes, email, allowed_signers } => {
            let s = unlock(cli, root)?;
            let vid = s.resolve_vault(vault)?;
            let iid = s.resolve_item(&vid, query)?;
            let (_, r) = s.read_record(&vid, &iid)?;
            let k = SshKey::from_record(&r)?;
            ssh::check_git_signing_support(&k)?;

            let socket_path = socket.clone().unwrap_or_else(ssh::default_ssh_agent_socket_path);
            let allowed_signers_path = allowed_signers.clone().unwrap_or_else(ssh::default_allowed_signers_path);
            let email_val = ssh::resolve_signing_email(email.as_deref(), &k);

            let cfg = ssh::build_git_signing_config(&k, &email_val, &allowed_signers_path, &socket_path)?;

            let should_apply = *apply;
            if !should_apply {
                println!("{}", cfg.export_auth_sock);
                println!();
                println!("{}", cfg.config_lines);
                return Ok(());
            }

            let is_global = !*local;
            let target_desc = if is_global { "global git config (~/.gitconfig)" } else { "local repository git config" };
            let signers_disp = ssh::display_path(&allowed_signers_path);

            if !*yes && !cli.yes {
                let prompt_msg = format!("Apply Git signing configuration to {target_desc} and update {signers_disp}?");
                if !confirm(&prompt_msg)? {
                    bail!("aborted (pass --yes to confirm)");
                }
            }

            ssh::apply_git_signing(&k, &email_val, &allowed_signers_path, &socket_path, is_global)?;
            eprintln!("Applied Git commit signing configuration to {target_desc}.");
            eprintln!("Updated allowed signers: {signers_disp}");
            if !cfg.auth_sock_active {
                eprintln!();
                eprintln!("Warning: SSH_AUTH_SOCK does not point to the gitpass SSH agent.");
                eprintln!("Git uses ssh-keygen to sign commits, which talks to $SSH_AUTH_SOCK.");
                eprintln!("Add this line to your shell profile (~/.zshrc or ~/.bashrc):");
                eprintln!("  {}", cfg.export_auth_sock);
            }
            println!("{}", cfg.export_auth_sock);
            println!();
            println!("{}", cfg.config_lines);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_fields_are_classified_as_secret_or_plain() {
        assert_eq!(field_markers(Field::Password), Some(PasteboardMarkers::SECRET));
        assert_eq!(field_markers(Field::Totp), Some(PasteboardMarkers::SECRET));
        for f in [Field::Username, Field::Url, Field::Notes, Field::Title] {
            assert_eq!(field_markers(f), None);
        }
    }
}

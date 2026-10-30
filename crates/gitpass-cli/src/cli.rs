use anyhow::Result;
use clap::{Parser, Subcommand};
use gitpass_core::item::Category;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gitpass", version, about = "Zero-knowledge password manager synced via a private git repo")]
pub(crate) struct Cli {
    /// Vault directory (default ~/.gitpass, or $GITPASS_PATH)
    #[arg(long, global = true, env = "GITPASS_PATH")]
    pub(crate) path: Option<PathBuf>,
    /// Accept a failed integrity/rollback check and re-seal the manifest from what is on disk (dangerous)
    #[arg(long, global = true)]
    pub(crate) accept_rollback: bool,
    /// Answer yes to confirmation prompts (v1 migration, share, revoke, plaintext export)
    #[arg(long, global = true)]
    pub(crate) yes: bool,
    /// Name to register this device under (default: $GITPASS_DEVICE_NAME, else hostname)
    #[arg(long, global = true, env = "GITPASS_DEVICE_NAME")]
    pub(crate) device_name: Option<String>,
    #[command(subcommand)]
    pub(crate) cmd: Cmd,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum AddCategory {
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
pub(crate) enum CliCategory {
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
pub(crate) enum Cmd {
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
        /// List previous passwords with replacement dates
        #[arg(long)]
        password_history: bool,
    },
    /// Show an item or its password history
    Show {
        /// Item query (id, title, or <vault>/<item>)
        item: String,
        #[arg(hide = true)]
        item2: Option<String>,
        /// Vault containing the item
        #[arg(long)]
        vault: Option<String>,
        /// List previous passwords with replacement dates
        #[arg(long)]
        password_history: bool,
        /// Reveal concealed field values
        #[arg(long)]
        reveal: bool,
        /// Output JSON
        #[arg(long)]
        json: bool,
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
        /// Remind me to change this login's password every 30d, 90d, 180d or 365d (or `none` to clear)
        #[arg(long = "rotate-every", value_name = "30d|90d|180d|365d|none")]
        rotate_every: Option<String>,
        /// Add a related item link (by title or ID, repeatable)
        #[arg(long = "link", alias = "related", action = clap::ArgAction::Append)]
        link: Vec<String>,
        /// Remove a related item link (by title or ID, repeatable)
        #[arg(long = "unlink", alias = "unrelated", action = clap::ArgAction::Append)]
        unlink: Vec<String>,
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
    /// Version history of an item (no secrets). For previous passwords on a login item, see `gitpass show <item> --password-history`.
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
    /// Import items from another password manager's export file (a password-protected Bitwarden
    /// JSON asks for the export password: $GITPASS_IMPORT_PASSWORD or a prompt)
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
    /// totp_available, passkey_available, weak_ssh_key, unused, rotation_due, rotation_soon. Findings you ignored in the app are counted but only
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
pub(crate) enum ShellTarget {
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

pub(crate) fn print_completions(shell: ShellTarget) {
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
pub(crate) enum ExportFmt { GitpassJson, Csv }

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum ListSort { Title, Recent }

pub(crate) struct UreqHibp(pub(crate) ureq::Agent);
impl gitpass_core::audit::PwnedRangeClient for UreqHibp {
    fn range(&self, prefix: &str) -> Result<String> {
        let r = self.0.get(&format!("{}{prefix}", gitpass_core::audit::HIBP_RANGE_URL))
            .set("Add-Padding", "true").set("User-Agent", concat!("gitpass/", env!("CARGO_PKG_VERSION"))).call()
            .map_err(|e| anyhow::anyhow!("HIBP request failed: {e}"))?;
        Ok(r.into_string()?)
    }
}

#[derive(Subcommand)]
pub(crate) enum DeviceCmd {
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
pub(crate) enum RemoteCmd {
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
pub(crate) enum GenSubcommand {
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
pub(crate) enum GenHistoryAction {
    /// Clear all generator history
    Clear,
}

#[derive(Subcommand)]
pub(crate) enum SshCmd {
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
pub(crate) enum AttachCmd {
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
pub(crate) struct CliAttachment {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) mime: String,
}

#[derive(Subcommand)]
pub(crate) enum IdentityCmd {
    /// Print this account's public key + fingerprint (creates the identity on first use; upgrades to format v4)
    Show,
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum Field { Password, Username, Url, Totp, Notes, Title }

#[derive(Subcommand)]
pub(crate) enum VaultCmd {
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

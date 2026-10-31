// SPDX-License-Identifier: AGPL-3.0-or-later
//! Path validation (S-FILE-04) and display form (R-OPEN-12). The platform layer normalizes with
//! `GetFullPathNameW` first and passes the result here.

/// Why a path was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    /// Empty path.
    Empty,
    /// Embedded NUL.
    Nul,
    /// Device namespace (`\\.\`, `\\?\GLOBALROOT`, `\??\`).
    DeviceNamespace,
    /// Reserved device name (`CON`, `NUL`, `COM1`, …) in any component.
    ReservedName,
    /// Alternate data stream (`:` beyond the drive letter).
    AlternateStream,
}

/// DOS device names.
const RESERVED: [&str; 6] = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];

/// `component` is a DOS device name with any extension (S3, prompt 022): `CON`, `PRN`, `AUX`,
/// `NUL`, `CONIN$`, `CONOUT$`, `COM1`–`COM9`, `LPT1`–`LPT9` and the superscript forms
/// `COM¹²³` / `LPT¹²³` (`nul.txt`, `Com¹.md`, …). The one check for every caller.
pub fn is_reserved(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches([' ', '.']);
    let u = stem.to_ascii_uppercase();
    if RESERVED.contains(&u.as_str()) {
        return true;
    }
    // COM1–9 / LPT1–9, including the superscript digits Windows also treats as devices.
    let b = u.as_bytes();
    (u.starts_with("COM") || u.starts_with("LPT"))
        && (b.len() == 4 && b.get(3).is_some_and(|d| (b'1'..=b'9').contains(d))
            || matches!(u.get(3..), Some("¹" | "²" | "³")))
}

/// Strips `\\?\` / `\\?\UNC\` for display (R-OPEN-12).
pub fn display_form(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = path.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        path.to_string()
    }
}

/// Internal long-path form: `C:\…` → `\\?\C:\…`, `\\server\share\…` → `\\?\UNC\server\share\…`.
pub fn long_form(path: &str) -> String {
    if path.starts_with(r"\\?\") {
        path.to_string()
    } else if let Some(rest) = path.strip_prefix(r"\\") {
        format!(r"\\?\UNC\{rest}")
    } else {
        format!(r"\\?\{path}")
    }
}

/// Validates a full (already normalized) path. User-opened UNC paths are allowed.
pub fn validate(path: &str) -> Result<(), PathError> {
    if path.is_empty() {
        return Err(PathError::Empty);
    }
    if path.contains('\0') {
        return Err(PathError::Nul);
    }
    let p = path.replace('/', "\\");
    let upper = p.to_ascii_uppercase();
    if upper.starts_with(r"\\.\")
        || upper.starts_with(r"\??\")
        || upper.starts_with(r"\\?\GLOBALROOT")
    {
        return Err(PathError::DeviceNamespace);
    }
    let body = display_form(&p);
    // Skip the drive designator `C:` when checking for stream colons.
    let after_drive = match body.as_bytes() {
        [d, b':', ..] if d.is_ascii_alphabetic() => body.get(2..).unwrap_or(""),
        _ => body.as_str(),
    };
    if after_drive.contains(':') {
        return Err(PathError::AlternateStream);
    }
    if after_drive.split('\\').any(is_reserved) {
        return Err(PathError::ReservedName);
    }
    Ok(())
}

/// `true` for a network form (UNC `\\server\share`, `//server/share`, `\\?\UNC\…`, WebDAV
/// `\\host@SSL\…`): S-RES-02 blocks these before any file access (NTLM leak).
pub fn is_network_form(path: &str) -> bool {
    let p = path.replace('/', "\\");
    let u = p.to_ascii_uppercase();
    if u.starts_with(r"\\?\UNC\") {
        return true;
    }
    u.starts_with(r"\\") && !u.starts_with(r"\\?\") && !u.starts_with(r"\\.\")
}

/// Lexical rules for the custom app icon (R-SYS-08, HARD-01): a valid full path (S-FILE-04),
/// local (no network form, S-RES-02) and ending in `.ico`. The platform layer adds the
/// mapped-network-drive check, the size limit and the ICO directory parse.
pub fn icon_candidate(full: &str) -> bool {
    validate(full).is_ok() && !is_network_form(full) && extension(full) == "ico"
}

/// Lower-case extension (ASCII, R-I18N-04) of a path, without the dot.
pub fn extension(path: &str) -> String {
    let name = path.rsplit(['\\', '/']).next().unwrap_or("");
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// File name component.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path)
}

/// S2 (prompt 022): a wikilink target name may name a sub-path (`sub/Page`) but never climb
/// (`..`), carry a drive or stream (`:`) or start at a root (`\`, `/`).
pub fn wiki_name_ok(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && !name.contains(':')
        && !name.starts_with(['\\', '/'])
        && !name.contains('\0')
}

// ---- S-RES-02 network / reparse policy (S1, prompt 022) ----------------------------------------

/// Result of `GetDriveTypeW` (only the classes the policy needs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    /// Fixed, removable, RAM disk or CD.
    Local,
    /// `DRIVE_REMOTE` (mapped network drive).
    Remote,
    /// Unknown / no root.
    Unknown,
}

/// The file-system layer used by the policy. The Windows implementation lives in
/// `yamdv-platform-win`; tests use a recording mock.
pub trait Fs {
    /// `GetDriveTypeW` for a root such as `C:\`.
    fn drive_kind(&mut self, root: &str) -> DriveKind;
    /// Opens the final component with `FILE_FLAG_OPEN_REPARSE_POINT` (never following it);
    /// returns the reparse target when it is a symlink/junction, `Some("")` for other
    /// reparse points, `None` when it is not a reparse point.
    fn reparse_target(&mut self, path: &str) -> Option<String>;
    /// File size, `None` if missing.
    fn size(&mut self, path: &str) -> Option<u64>;
}

/// A file system that answers "local, not a reparse point, missing" without any I/O
/// (fuzzing and pure layout).
#[derive(Debug, Default)]
pub struct NoFs;

impl Fs for NoFs {
    fn drive_kind(&mut self, _: &str) -> DriveKind {
        DriveKind::Local
    }
    fn reparse_target(&mut self, _: &str) -> Option<String> {
        None
    }
    fn size(&mut self, _: &str) -> Option<u64> {
        None
    }
}

/// Who asked for a file-system entry (S1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope<'a> {
    /// The user picked it himself (Open dialog, CLI, Explorer, drop, History): it may be on a
    /// network share. Only the lexical checks apply.
    User,
    /// Referenced by the document at this full path (links, wikilinks, images): local, or on the
    /// document's own server and share (UNC) / mapped drive.
    Doc(&'a str),
    /// Never on the network (custom icon, Files tab roots that are not the document's).
    Local,
}

/// Why [`local_policy`] refused a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// Lexically invalid (S-FILE-04).
    Invalid(PathError),
    /// UNC / WebDAV / mapped network drive / reparse point to the network (S-RES-02).
    Network,
    /// The drive does not exist (`GetDriveTypeW` unknown); nothing to open.
    Missing,
}

/// `\\server\share` (lower-case) of a UNC path, `None` for other forms.
pub fn unc_share(p: &str) -> Option<String> {
    let rest = p.strip_prefix("\\\\")?;
    let mut it = rest.split('\\');
    let host = it.next()?;
    let share = it.next()?;
    if host.is_empty() || share.is_empty() || host == "?" || host == "." {
        return None;
    }
    Some(format!("\\\\{host}\\{share}").to_lowercase())
}

/// Backslashes, no `\\?\` / `\\?\UNC\` prefix, `\??\UNC\x` → `\\x`, `\??\C:` → `C:`.
fn canonical(path: &str) -> String {
    let p = path.replace('/', "\\");
    let p = if p.len() >= 8
        && p.get(..8)
            .is_some_and(|h| h.eq_ignore_ascii_case("\\??\\UNC\\"))
    {
        format!("\\\\{}", p.get(8..).unwrap_or(""))
    } else if let Some(r) = p.strip_prefix("\\??\\") {
        r.to_string()
    } else {
        p
    };
    if p.len() >= 8
        && p.get(..8)
            .is_some_and(|h| h.eq_ignore_ascii_case("\\\\?\\UNC\\"))
    {
        format!("\\\\{}", p.get(8..).unwrap_or(""))
    } else if let Some(r) = p.strip_prefix("\\\\?\\") {
        r.to_string()
    } else {
        p
    }
}

/// The share or mapped drive a [`Scope::Doc`] document lives on.
struct Home {
    share: Option<String>,
    drive: Option<u8>,
}

impl Home {
    fn of(scope: Scope<'_>) -> Self {
        let Scope::Doc(d) = scope else {
            return Self {
                share: None,
                drive: None,
            };
        };
        let d = canonical(d);
        let share = unc_share(&d).filter(|s| !s.contains('@'));
        let drive = match d.as_bytes() {
            [l, b':', ..] if l.is_ascii_alphabetic() => Some(l.to_ascii_uppercase()),
            _ => None,
        };
        Self { share, drive }
    }
    /// A network target (UNC form or a mapped drive letter) is allowed only on the home.
    fn allows_unc(&self, p: &str) -> bool {
        let s = unc_share(p);
        s.is_some()
            && !p
                .get(2..)
                .unwrap_or("")
                .split('\\')
                .next()
                .unwrap_or("@")
                .contains('@')
            && s == self.share
    }
    fn allows_drive(&self, letter: u8) -> bool {
        self.drive == Some(letter.to_ascii_uppercase())
    }
}

/// Checks one drive-letter path: a mapped network drive is allowed only when it is the
/// document's own drive.
fn check_drive(p: &str, home: &Home, fs: &mut dyn Fs) -> Result<(), Blocked> {
    let Some(&l) = p.as_bytes().first() else {
        return Err(Blocked::Network);
    };
    let root = format!("{}\\", p.get(..2).unwrap_or(""));
    match fs.drive_kind(&root) {
        DriveKind::Local => Ok(()),
        DriveKind::Remote if home.allows_drive(l) => Ok(()),
        DriveKind::Remote => Err(Blocked::Network),
        DriveKind::Unknown => Err(Blocked::Missing),
    }
}

fn is_drive_path(p: &str) -> bool {
    matches!(p.as_bytes(), [l, b':', b'\\', ..] | [l, b':'] if l.is_ascii_alphabetic())
}

/// S-RES-02 for **every** file-system entry (S1, prompt 022): the lexical checks of
/// [`validate`], then, unless the user picked the path himself ([`Scope::User`]): network forms
/// (UNC, `\\?\UNC\`, WebDAV `host@SSL`) are refused unless on the document's own server and
/// share; a mapped network drive (`DRIVE_REMOTE`) is refused unless it is the document's drive;
/// and every component is checked for a reparse point **before** anything below it is touched,
/// so a junction or symlink to the network is refused before `CreateFile` follows it. Other
/// reparse points (unreadable target) are refused.
pub fn local_policy(path: &str, scope: Scope<'_>, fs: &mut dyn Fs) -> Result<(), Blocked> {
    validate(path).map_err(Blocked::Invalid)?;
    if scope == Scope::User {
        return Ok(());
    }
    let p = canonical(path);
    let home = Home::of(scope);
    let root_len = if is_network_form(&p) {
        if !home.allows_unc(&p) {
            return Err(Blocked::Network);
        }
        unc_share(&p).map_or(p.len(), |s| s.len())
    } else if is_drive_path(&p) {
        check_drive(&p, &home, fs)?;
        2
    } else {
        return Err(Blocked::Invalid(PathError::Empty));
    };
    // Walk the components from the root down; each prefix is examined without following it.
    let mut end = root_len;
    let bytes = p.as_bytes();
    while end < p.len() {
        let next = bytes
            .iter()
            .skip(end + 1)
            .position(|&b| b == b'\\')
            .map_or(p.len(), |i| end + 1 + i);
        let prefix = p.get(..next).unwrap_or(&p);
        end = next;
        if prefix.len() <= root_len + 1 {
            continue;
        }
        let Some(t) = fs.reparse_target(prefix) else {
            continue;
        };
        let t = canonical(&t);
        if t.is_empty() {
            return Err(Blocked::Network);
        }
        if t.starts_with("\\\\") {
            if !home.allows_unc(&t) {
                return Err(Blocked::Network);
            }
        } else if is_drive_path(&t) {
            check_drive(&t, &home, fs).map_err(|_| Blocked::Network)?;
        } else {
            // Volume GUID mount points and other targets: not provably local.
            return Err(Blocked::Network);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Records every file-system call.
    #[derive(Default)]
    struct Mock {
        calls: Vec<String>,
        remote: Vec<&'static str>,
        reparse: Vec<(&'static str, &'static str)>,
    }
    impl Fs for Mock {
        fn drive_kind(&mut self, root: &str) -> DriveKind {
            self.calls.push(format!("drive {root}"));
            if self.remote.iter().any(|d| root.eq_ignore_ascii_case(d)) {
                DriveKind::Remote
            } else {
                DriveKind::Local
            }
        }
        fn reparse_target(&mut self, path: &str) -> Option<String> {
            self.calls.push(format!("reparse {path}"));
            self.reparse
                .iter()
                .find(|(p, _)| p.eq_ignore_ascii_case(path))
                .map(|(_, t)| (*t).to_string())
        }
        fn size(&mut self, _: &str) -> Option<u64> {
            None
        }
    }

    const DOC: &str = r"C:\docs\a.md";

    #[test]
    fn s1_network_forms_blocked_before_fs() {
        for p in [
            r"\\server\share\x.md",
            r"\\?\UNC\server\share\x.md",
            r"//server/share/x.md",
            r"\\host@SSL\DavWWWRoot\x.md",
            r"\\host@SSL@8443\dav\x.md",
        ] {
            let mut m = Mock::default();
            assert_eq!(
                local_policy(p, Scope::Doc(DOC), &mut m),
                Err(Blocked::Network),
                "{p}"
            );
            assert!(m.calls.is_empty(), "{p}: {:?}", m.calls);
            let mut m = Mock::default();
            assert_eq!(
                local_policy(p, Scope::Local, &mut m),
                Err(Blocked::Network),
                "{p}"
            );
        }
        // The user may open a document on a share himself.
        let mut m = Mock::default();
        assert_eq!(
            local_policy(r"\\server\share\x.md", Scope::User, &mut m),
            Ok(())
        );
        assert!(m.calls.is_empty());
        assert_eq!(
            local_policy(r"C:\con.md", Scope::User, &mut m),
            Err(Blocked::Invalid(PathError::ReservedName))
        );
    }

    #[test]
    fn s1_mapped_drive() {
        let mut m = Mock {
            remote: vec![r"Z:\"],
            ..Mock::default()
        };
        assert_eq!(
            local_policy(r"Z:\x.md", Scope::Doc(DOC), &mut m),
            Err(Blocked::Network)
        );
        assert_eq!(m.calls, vec![r"drive Z:\".to_string()]);
        // Same mapped drive as the document: allowed.
        assert_eq!(
            local_policy(r"Z:\b\x.md", Scope::Doc(r"Z:\a.md"), &mut m),
            Ok(())
        );
        assert_eq!(
            local_policy(r"z:\x.ico", Scope::Local, &mut m),
            Err(Blocked::Network)
        );
    }

    #[test]
    fn s1_junction_to_unc_blocked_before_following() {
        let mut m = Mock {
            reparse: vec![(r"C:\docs\j", r"\??\UNC\evil\s")],
            ..Mock::default()
        };
        assert_eq!(
            local_policy(r"C:\docs\j\deep\x.md", Scope::Doc(DOC), &mut m),
            Err(Blocked::Network)
        );
        // Nothing below the junction was examined.
        assert!(
            !m.calls.iter().any(|c| c.contains(r"j\deep")),
            "{:?}",
            m.calls
        );
        let mut m = Mock {
            reparse: vec![(r"C:\docs\l.md", r"\\evil\s\x.md")],
            ..Mock::default()
        };
        assert_eq!(
            local_policy(r"C:\docs\l.md", Scope::Doc(DOC), &mut m),
            Err(Blocked::Network)
        );
        // A link to a mapped network drive, and an unreadable reparse point.
        let mut m = Mock {
            remote: vec![r"Y:\"],
            reparse: vec![(r"C:\docs\k", r"Y:\x"), (r"C:\docs\o.md", "")],
            ..Mock::default()
        };
        assert_eq!(
            local_policy(r"C:\docs\k\a.md", Scope::Doc(DOC), &mut m),
            Err(Blocked::Network)
        );
        assert_eq!(
            local_policy(r"C:\docs\o.md", Scope::Doc(DOC), &mut m),
            Err(Blocked::Network)
        );
        // A junction to another local folder is fine.
        let mut m = Mock {
            reparse: vec![(r"C:\docs\loc", r"\??\D:\notes")],
            ..Mock::default()
        };
        assert_eq!(
            local_policy(r"C:\docs\loc\a.md", Scope::Doc(DOC), &mut m),
            Ok(())
        );
        assert!(m.calls.contains(&r"drive D:\".to_string()));
    }

    #[test]
    fn s1_same_share_exception() {
        let doc = r"\\nas\team\docs\a.md";
        let mut m = Mock::default();
        assert_eq!(
            local_policy(r"\\nas\team\img\b.md", Scope::Doc(doc), &mut m),
            Ok(())
        );
        assert_eq!(
            local_policy(r"\\?\UNC\NAS\Team\b.md", Scope::Doc(doc), &mut m),
            Ok(())
        );
        for p in [
            r"\\nas\other\b.md",
            r"\\evil\team\b.md",
            r"\\nas@SSL\team\b.md",
        ] {
            assert_eq!(
                local_policy(p, Scope::Doc(doc), &mut m),
                Err(Blocked::Network),
                "{p}"
            );
        }
        // A junction on the share that leads to another server.
        let mut m = Mock {
            reparse: vec![(r"\\nas\team\j", r"\\evil\s")],
            ..Mock::default()
        };
        assert_eq!(
            local_policy(r"\\nas\team\j\x.md", Scope::Doc(doc), &mut m),
            Err(Blocked::Network)
        );
        // A WebDAV document has no same-share exception.
        let mut m = Mock::default();
        assert_eq!(
            local_policy(r"\\h@SSL\d\b.md", Scope::Doc(r"\\h@SSL\d\a.md"), &mut m),
            Err(Blocked::Network)
        );
    }

    #[test]
    fn s2_wiki_names() {
        for n in ["Page", "Page.md", "sub/Page", "sub\\Page.md", "a.b"] {
            assert!(wiki_name_ok(n), "{n}");
        }
        for n in [
            "",
            "../x",
            "a/../../x",
            "C:x",
            "x.md:ads",
            "\\\\srv\\s\\x",
            "/x",
            "\\x",
        ] {
            assert!(!wiki_name_ok(n), "{n}");
        }
    }

    #[test]
    fn s3_device_names_one_check() {
        for c in [
            "CON",
            "con.md",
            "PRN",
            "aux.txt",
            "NUL",
            "CONIN$",
            "conout$.md",
            "COM1",
            "lpt9.png",
            "COM¹",
            "com².md",
            "LPT³.txt",
            "nul .md",
        ] {
            assert!(is_reserved(c), "{c}");
        }
        for c in [
            "console.md",
            "COM0",
            "LPT10",
            "COM",
            "conin.md",
            "nullable.md",
            "COM⁴",
        ] {
            assert!(!is_reserved(c), "{c}");
        }
        assert_eq!(validate(r"C:\x\CONOUT$"), Err(PathError::ReservedName));
    }

    #[test]
    fn s_file_04_rejects() {
        assert_eq!(validate(""), Err(PathError::Empty));
        assert_eq!(validate("C:\\a\0b.md"), Err(PathError::Nul));
        assert_eq!(
            validate(r"\\.\PhysicalDrive0"),
            Err(PathError::DeviceNamespace)
        );
        assert_eq!(
            validate(r"\\?\GLOBALROOT\Device\X"),
            Err(PathError::DeviceNamespace)
        );
        assert_eq!(validate(r"\??\C:\x"), Err(PathError::DeviceNamespace));
        assert_eq!(validate(r"C:\docs\con.md"), Err(PathError::ReservedName));
        assert_eq!(validate(r"C:\docs\Lpt9"), Err(PathError::ReservedName));
        assert_eq!(validate(r"C:\docs\COM¹.txt"), Err(PathError::ReservedName));
        assert_eq!(
            validate(r"C:\docs\a.md:secret"),
            Err(PathError::AlternateStream)
        );
        assert_eq!(
            validate(r"\\?\C:\docs\a.md:x"),
            Err(PathError::AlternateStream)
        );
    }

    #[test]
    fn s_file_04_accepts() {
        assert_eq!(validate(r"C:\docs\readme.md"), Ok(()));
        assert_eq!(validate(r"C:\docs\console.md"), Ok(()));
        assert_eq!(validate(r"C:\docs\COM0.md"), Ok(()));
        assert_eq!(validate(r"\\server\share\a.md"), Ok(()));
        assert_eq!(validate(r"\\?\UNC\server\share\a.md"), Ok(()));
        assert_eq!(validate(r"C:/x/y.md"), Ok(()));
    }

    #[test]
    fn r_open_12_forms() {
        assert_eq!(display_form(r"\\?\C:\a"), r"C:\a");
        assert_eq!(display_form(r"\\?\UNC\s\sh\a"), r"\\s\sh\a");
        assert_eq!(long_form(r"C:\a"), r"\\?\C:\a");
        assert_eq!(long_form(r"\\s\sh\a"), r"\\?\UNC\s\sh\a");
        assert_eq!(extension(r"C:\x\Readme.MD"), "md");
        assert_eq!(file_name(r"C:\x\Readme.MD"), "Readme.MD");
    }
}

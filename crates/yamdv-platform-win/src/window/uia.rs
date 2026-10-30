// SPDX-License-Identifier: AGPL-3.0-or-later
//! UI Automation provider (prompt 019 §4, FUT-47 companion): Simple / Fragment / FragmentRoot
//! providers for the window, title-bar buttons, banners, sidebar tabs and rows, the image-viewer
//! toolbar, the settings panel, open menus and the document (a Document control with a
//! read-only TextPattern). Nothing exists until a client asks: `UIAutomationCore.dll` is loaded
//! from System32 on the first `WM_GETOBJECT(UiaRootObjectId)` (M0/M1 unchanged), and the element
//! tree is rebuilt from the window state on every call (no cache).
//!
//! F-42 (prompt 020, R-A11Y-03): control patterns — Invoke (buttons, menu items), Toggle
//! (toggles), Value / RangeValue (text inputs, dropdown values, sliders), ExpandCollapse (the
//! More menu, submenus, dropdowns, outline sections) and SelectionItem (tabs, list items). A
//! pattern object exists only while a client holds it; acting on one posts the same mouse
//! messages a click would (so every action runs through the normal input path, outside any
//! borrow). Without a client nothing of this runs (zero cost).
#![allow(unsafe_code)]

use super::*;
use std::cell::Cell;
use std::ffi::c_void;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::mem::ManuallyDrop;
use std::sync::OnceLock;
use windows::Win32::Foundation::{E_INVALIDARG, HMODULE};
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
use windows::Win32::System::Variant::{
    VARENUM, VARIANT, VT_BOOL, VT_BSTR, VT_I4, VT_R8, VT_UNKNOWN,
};
use windows::Win32::UI::Accessibility as ua;
use windows::Win32::UI::Accessibility::{
    IRawElementProviderFragment, IRawElementProviderFragment_Impl, IRawElementProviderFragmentRoot,
    IRawElementProviderFragmentRoot_Impl, IRawElementProviderSimple,
    IRawElementProviderSimple_Impl, ITextProvider, ITextProvider_Impl, ITextRangeProvider,
    ITextRangeProvider_Impl, UiaPoint, UiaRect,
};
use windows::core::{AsImpl, BOOL, BSTR, HRESULT, IUnknown, Interface, PCSTR, Ref, implement, s};
use yamdv_core::chrome as ch;
use yamdv_ui::a11y::{self, Unit};

type ReturnFn = unsafe extern "system" fn(HWND, WPARAM, LPARAM, *mut c_void) -> LRESULT;
type HostFn = unsafe extern "system" fn(HWND, *mut *mut c_void) -> HRESULT;
type RaiseFn = unsafe extern "system" fn(*mut c_void, i32) -> HRESULT;
type ListenFn = unsafe extern "system" fn() -> BOOL;
type NotSupFn = unsafe extern "system" fn(*mut *mut c_void) -> HRESULT;

struct Api {
    ret: ReturnFn,
    host: HostFn,
    raise: RaiseFn,
    listen: ListenFn,
    notsup: NotSupFn,
}

static API: OnceLock<Option<Api>> = OnceLock::new();

/// `UIA_E_INVALIDOPERATION` (read-only text: no selection).
const E_INVALIDOP: HRESULT = HRESULT(0x8013_1509_u32 as i32);
/// `UIA_E_ELEMENTNOTAVAILABLE` (the element left the tree).
const E_GONE: HRESULT = HRESULT(0x8004_0201_u32 as i32);

fn load() -> Option<Api> {
    // SAFETY: System32-only search; the module stays loaded for the process lifetime.
    let h: HMODULE = unsafe {
        LoadLibraryExW(
            windows::core::w!("UIAutomationCore.dll"),
            None,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    }
    .ok()?;
    let f = |n: PCSTR| {
        // SAFETY: valid module handle and NUL-terminated name.
        unsafe { GetProcAddress(h, n) }
    };
    let (r, ho, ra, li, ns) = (
        f(s!("UiaReturnRawElementProvider"))?,
        f(s!("UiaHostProviderFromHwnd"))?,
        f(s!("UiaRaiseAutomationEvent"))?,
        f(s!("UiaClientsAreListening"))?,
        f(s!("UiaGetReservedNotSupportedValue"))?,
    );
    type Raw = unsafe extern "system" fn() -> isize;
    // SAFETY: the documented signatures of these UIAutomationCore exports.
    unsafe {
        Some(Api {
            ret: std::mem::transmute::<Raw, ReturnFn>(r),
            host: std::mem::transmute::<Raw, HostFn>(ho),
            raise: std::mem::transmute::<Raw, RaiseFn>(ra),
            listen: std::mem::transmute::<Raw, ListenFn>(li),
            notsup: std::mem::transmute::<Raw, NotSupFn>(ns),
        })
    }
}

fn loaded() -> Option<&'static Api> {
    API.get().and_then(Option::as_ref)
}

/// `WM_GETOBJECT`: answers `UiaRootObjectId` only (loads UIAutomationCore on first use).
pub(super) fn on_getobject(hwnd: HWND, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    if (lp.0 as u32) as i32 != ua::UiaRootObjectId {
        return None;
    }
    let api = API.get_or_init(load).as_ref()?;
    let root: IRawElementProviderSimple = Root { hwnd }.into();
    // SAFETY: valid provider; UIA takes its own reference.
    Some(unsafe { (api.ret)(hwnd, wp, lp, root.as_raw()) })
}

/// `WM_DESTROY`: disconnects the providers of `hwnd` (only if UIA was ever used).
pub(super) fn on_destroy(hwnd: HWND) {
    if let Some(a) = loaded() {
        // SAFETY: documented disconnect call (null provider).
        let _ = unsafe { (a.ret)(hwnd, WPARAM(0), LPARAM(0), std::ptr::null_mut()) };
    }
}

/// Focus changed (`WM_SETFOCUS`, settings/sidebar/title-bar focus moves): raises
/// `UIA_AutomationFocusChangedEventId` on the focused element when a client listens.
pub(super) fn focus_changed(w: &Win, sh: &Shared) {
    let Some(a) = loaded() else { return };
    // SAFETY: no preconditions.
    if !unsafe { (a.listen)() }.as_bool() {
        return;
    }
    let t = w.uia_tree(sh);
    let Some(n) = t.nodes.iter().find(|n| n.focus) else {
        return;
    };
    let el: IRawElementProviderSimple = Elem {
        hwnd: w.hwnd,
        key: n.key,
    }
    .into();
    // SAFETY: valid provider.
    let _ = unsafe { (a.raise)(el.as_raw(), ua::UIA_AutomationFocusChangedEventId.0) };
}

/// Element kinds (UIA control types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum K {
    Pane,
    Document,
    TitleBar,
    Button,
    Tab,
    TabItem,
    List,
    ListItem,
    Text,
    CheckBox,
    ComboBox,
    Slider,
    Edit,
    Menu,
    MenuItem,
    ToolBar,
}

impl K {
    fn control_type(self) -> i32 {
        let c = match self {
            K::Pane => ua::UIA_PaneControlTypeId,
            K::Document => ua::UIA_DocumentControlTypeId,
            K::TitleBar => ua::UIA_TitleBarControlTypeId,
            K::Button => ua::UIA_ButtonControlTypeId,
            K::Tab => ua::UIA_TabControlTypeId,
            K::TabItem => ua::UIA_TabItemControlTypeId,
            K::List => ua::UIA_ListControlTypeId,
            K::ListItem => ua::UIA_ListItemControlTypeId,
            K::Text => ua::UIA_TextControlTypeId,
            K::CheckBox => ua::UIA_CheckBoxControlTypeId,
            K::ComboBox => ua::UIA_ComboBoxControlTypeId,
            K::Slider => ua::UIA_SliderControlTypeId,
            K::Edit => ua::UIA_EditControlTypeId,
            K::Menu => ua::UIA_MenuControlTypeId,
            K::MenuItem => ua::UIA_MenuItemControlTypeId,
            K::ToolBar => ua::UIA_ToolBarControlTypeId,
        };
        c.0
    }

    fn focusable(self) -> bool {
        !matches!(
            self,
            K::Pane | K::TitleBar | K::Tab | K::List | K::Text | K::Menu | K::ToolBar
        )
    }
}

/// Where an element is: client DIPs or screen pixels (popup menus).
#[derive(Debug, Clone, Copy)]
pub(super) enum At {
    Dip(Rect),
    Px(ch::Px),
}

/// How a ValuePattern may change its text (F-42).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Edit {
    ReadOnly,
    /// Click, replace the text with keys.
    Keys,
    /// The same, then Enter commits (settings text rows).
    KeysEnter,
}

/// Control pattern of an element (F-42).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) enum Pat {
    #[default]
    None,
    Invoke,
    Toggle(bool),
    Select(bool),
    /// ExpandCollapse (true = expanded); expanding or collapsing clicks the hit rect.
    Expand(bool),
    /// RangeValue: value, minimum, maximum, step; `SetValue` clicks the matching x of the hit rect.
    Range(f64, f64, f64, f64),
}

#[derive(Debug, Clone)]
pub(super) struct Node {
    pub(super) kind: K,
    /// Control pattern (F-42) and the ValuePattern text (`None` = no ValuePattern).
    pub(super) pat: Pat,
    pub(super) value: Option<(String, Edit)>,
    /// Where a synthesized click lands (default: the element itself) and the popup window that
    /// owns a screen-pixel element (menus).
    pub(super) hit: Option<At>,
    pub(super) target: Option<HWND>,
    pub(super) name: String,
    pub(super) at: At,
    pub(super) parent: usize,
    pub(super) focus: bool,
    /// Stable identity (parent, kind, name, sibling ordinal): runtime id and lookups.
    pub(super) key: u64,
}

/// A snapshot of the element tree; node 0 is the window (hosted by the HWND provider).
pub(super) struct Tree {
    pub(super) nodes: Vec<Node>,
}

impl Tree {
    fn new() -> Self {
        Self {
            nodes: vec![Node {
                kind: K::Pane,
                pat: Pat::None,
                value: None,
                hit: None,
                target: None,
                name: String::new(),
                at: At::Dip(Rect::new(0.0, 0.0, 0.0, 0.0)),
                parent: 0,
                focus: false,
                key: 0,
            }],
        }
    }

    /// Adds a child of `parent`; returns its index.
    pub(super) fn add(&mut self, parent: usize, kind: K, name: &str, at: At) -> usize {
        let ord = self
            .nodes
            .iter()
            .skip(1)
            .filter(|n| n.parent == parent && n.kind == kind && n.name == name)
            .count();
        let mut h = DefaultHasher::new();
        self.nodes.get(parent).map_or(0, |p| p.key).hash(&mut h);
        kind.hash(&mut h);
        name.hash(&mut h);
        ord.hash(&mut h);
        let key = h.finish() | 1;
        let pat = match kind {
            K::Button | K::MenuItem => Pat::Invoke,
            K::CheckBox => Pat::Toggle(false),
            K::TabItem | K::ListItem => Pat::Select(false),
            K::ComboBox => Pat::Expand(false),
            _ => Pat::None,
        };
        self.nodes.push(Node {
            kind,
            pat,
            value: None,
            hit: None,
            target: None,
            name: name.to_string(),
            at,
            parent,
            focus: false,
            key,
        });
        self.nodes.len() - 1
    }

    /// Marks node `i` as the keyboard focus (the first mark wins).
    pub(super) fn focus(&mut self, i: usize) {
        if self.nodes.iter().any(|n| n.focus) {
            return;
        }
        if let Some(n) = self.nodes.get_mut(i) {
            n.focus = true;
        }
    }

    /// Sets the control pattern of node `i` (F-42).
    pub(super) fn pat(&mut self, i: usize, p: Pat) {
        if let Some(n) = self.nodes.get_mut(i) {
            n.pat = p;
        }
    }

    /// Gives node `i` a ValuePattern.
    pub(super) fn value(&mut self, i: usize, v: String, e: Edit) {
        if let Some(n) = self.nodes.get_mut(i) {
            n.value = Some((v, e));
        }
    }

    /// Where clicks on node `i` land, and the popup window owning a screen-pixel rect.
    pub(super) fn hit(&mut self, i: usize, at: At, target: Option<HWND>) {
        if let Some(n) = self.nodes.get_mut(i) {
            n.hit = Some(at);
            n.target = target;
        }
    }

    fn find(&self, key: u64) -> Option<usize> {
        self.nodes.iter().position(|n| n.key == key)
    }

    fn children(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        self.nodes
            .iter()
            .enumerate()
            .skip(1)
            .filter(move |(_, n)| n.parent == i)
            .map(|(j, _)| j)
    }
}

/// English / Turkish name.
pub(super) fn pick(lang: Lang, en: &'static str, tr: &'static str) -> &'static str {
    match lang {
        Lang::En => en,
        Lang::Tr => tr,
    }
}

impl Win {
    /// The element tree of this window (client DIPs; content-area helpers add `self.top`).
    pub(super) fn uia_tree(&self, sh: &Shared) -> Tree {
        let mut t = Tree::new();
        let l = sh.lang;
        // The document first: lowest in z-order for hit testing.
        let doc = t.add(
            0,
            K::Document,
            pick(l, "Document", "Belge"),
            At::Dip(Rect::new(0.0, self.top, self.cw, self.ch)),
        );
        if !self.fs {
            let bar = t.add(
                0,
                K::TitleBar,
                pick(l, "Title bar", "Başlık çubuğu"),
                At::Dip(Rect::new(0.0, 0.0, self.cw, ch::BAR_H)),
            );
            for (i, (p, x, w)) in ch::buttons(self.cw).into_iter().enumerate() {
                let name = match p {
                    ch::Part::Settings => pick(l, "Settings", "Ayarlar"),
                    ch::Part::More => pick(l, "More options", "Diğer seçenekler"),
                    ch::Part::Min => pick(l, "Minimize", "Simge durumuna küçült"),
                    ch::Part::Max => pick(l, "Maximize", "Ekranı kapla"),
                    _ => pick(l, "Close", "Kapat"),
                };
                let b = t.add(
                    bar,
                    K::Button,
                    name,
                    At::Dip(Rect::new(x, 0.0, w, ch::BAR_H)),
                );
                if p == ch::Part::More {
                    t.pat(b, Pat::Expand(popup::owned_by(self.hwnd)));
                }
                if self.nav.kbd
                    && self.nav.region == nav::Region::TitleBar
                    && self.nav.chrome_focus == i
                {
                    t.focus(b);
                }
            }
        }
        self.uia_toolbar(sh, &mut t);
        self.tz_uia(sh, &mut t);
        self.uia_sidebar(sh, &mut t);
        self.uia_settings(sh, &mut t);
        popup::uia_menus(self.hwnd, &mut t);
        t.focus(doc);
        t
    }

    fn uia_sidebar(&self, sh: &Shared, t: &mut Tree) {
        let Some(sr) = self.side_rect(sh) else { return };
        if !self.side_shown() {
            return;
        }
        let l = sh.lang;
        let off = |r: Rect| At::Dip(Rect::new(r.x, r.y + self.top, r.w, r.h));
        let pane = t.add(0, K::Pane, pick(l, "Sidebar", "Kenar çubuğu"), off(sr));
        let rows = self.side_rows(sh);
        let tabs: Vec<&sidebar::Row> = rows
            .iter()
            .filter(|r| matches!(r.kind, sidebar::RowKind::Tab(_)))
            .collect();
        if !tabs.is_empty() {
            let tab = t.add(
                pane,
                K::Tab,
                pick(l, "Sidebar tabs", "Kenar sekmeleri"),
                off(sr),
            );
            let cur = app::with(|a| a.state.sidebar_tab).unwrap_or(0);
            for r in tabs {
                let n = t.add(tab, K::TabItem, &r.text, off(r.rect));
                t.pat(
                    n,
                    Pat::Select(matches!(r.kind, sidebar::RowKind::Tab(i) if i == cur)),
                );
            }
        }
        let list = t.add(pane, K::List, pick(l, "Items", "Öğeler"), off(sr));
        for r in rows
            .iter()
            .filter(|r| !matches!(r.kind, sidebar::RowKind::Tab(_)))
        {
            let k = if matches!(r.kind, sidebar::RowKind::Hint(_)) {
                K::Text
            } else {
                K::ListItem
            };
            let n = t.add(list, k, &r.text, off(r.rect));
            // F-42: outline sections and folders expand / collapse (the fold arrow for
            // sections); other rows are selectable.
            match &r.kind {
                sidebar::RowKind::Toc {
                    kids: true, open, ..
                } => {
                    t.pat(n, Pat::Expand(*open));
                    t.hit(n, off(Rect::new(r.rect.x, r.rect.y, 16.0, r.rect.h)), None);
                }
                sidebar::RowKind::Toc { active, .. } => t.pat(n, Pat::Select(*active)),
                sidebar::RowKind::File {
                    dir: true, open, ..
                } => t.pat(n, Pat::Expand(*open)),
                sidebar::RowKind::File { current, .. } | sidebar::RowKind::Hist { current, .. } => {
                    t.pat(n, Pat::Select(*current));
                }
                sidebar::RowKind::Hint(_) => t.pat(n, Pat::None),
                _ => {}
            }
        }
    }

    /// Reading text of the document (UTF-16), the TextPattern content.
    fn uia_text(&self) -> Vec<u16> {
        let View::Doc(d) = &self.view else {
            return Vec::new();
        };
        let src: std::borrow::Cow<'_, str> = match self.edit() {
            Some(e) => std::borrow::Cow::Owned(e.ed.text()),
            None => std::borrow::Cow::Borrowed(d.content.source()),
        };
        ch::reading_text(&src).encode_utf16().collect()
    }

    fn uia_screen(&self, at: At) -> UiaRect {
        match at {
            At::Px((l, t, r, b)) => UiaRect {
                left: f64::from(l),
                top: f64::from(t),
                width: f64::from(r - l),
                height: f64::from(b - t),
            },
            At::Dip(r) => {
                let mut o = POINT::default();
                // SAFETY: valid hwnd and out-pointer.
                let _ = unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(self.hwnd, &mut o) };
                let s = f64::from(self.scale);
                UiaRect {
                    left: f64::from(o.x) + f64::from(r.x) * s,
                    top: f64::from(o.y) + f64::from(r.y) * s,
                    width: f64::from(r.w) * s,
                    height: f64::from(r.h) * s,
                }
            }
        }
    }
}

/// Runs `f` on the window state (read-only); `None` while the window is busy or gone.
fn with<T>(hwnd: HWND, f: impl FnOnce(&Win, &Shared) -> T) -> Option<T> {
    let win = WINDOWS.with(|m| m.borrow().get(&(hwnd.0 as isize)).cloned())?;
    let sh = shared()?;
    let w = win.try_borrow().ok()?;
    Some(f(&w, &sh))
}

fn gone() -> windows::core::Error {
    E_GONE.into()
}

fn none() -> windows::core::Error {
    windows::core::Error::empty()
}

fn v_i4(v: i32) -> VARIANT {
    let mut x = VARIANT::default();
    // SAFETY: plain union writes of a fresh VARIANT.
    unsafe {
        let a = &mut x.Anonymous.Anonymous;
        a.vt = VT_I4;
        a.Anonymous.lVal = v;
    }
    x
}

fn v_bool(v: bool) -> VARIANT {
    let mut x = VARIANT::default();
    // SAFETY: plain union writes of a fresh VARIANT.
    unsafe {
        let a = &mut x.Anonymous.Anonymous;
        a.vt = VT_BOOL;
        a.Anonymous.boolVal = windows::Win32::Foundation::VARIANT_BOOL(if v { -1 } else { 0 });
    }
    x
}

fn v_str(v: &str) -> VARIANT {
    let mut x = VARIANT::default();
    // SAFETY: the caller (UIA) owns and frees the BSTR.
    unsafe {
        let a = &mut x.Anonymous.Anonymous;
        a.vt = VT_BSTR;
        a.Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(v));
    }
    x
}

fn v_unknown(u: IUnknown) -> VARIANT {
    let mut x = VARIANT::default();
    // SAFETY: ownership of `u` moves into the VARIANT (freed by the caller).
    unsafe {
        let a = &mut x.Anonymous.Anonymous;
        a.vt = VT_UNKNOWN;
        a.Anonymous.punkVal = ManuallyDrop::new(Some(u));
    }
    x
}

fn not_supported() -> VARIANT {
    let Some(a) = loaded() else {
        return VARIANT::default();
    };
    let mut p = std::ptr::null_mut();
    // SAFETY: out-pointer; the returned object is a static singleton (AddRef'd).
    if unsafe { (a.notsup)(&mut p) }.is_ok() && !p.is_null() {
        // SAFETY: `p` is an owned IUnknown.
        return v_unknown(unsafe { IUnknown::from_raw(p) });
    }
    VARIANT::default()
}

/// A one-dimensional SAFEARRAY of `vt`; `put` stores element `i` from a pointer.
fn array(vt: VARENUM, n: usize, put: impl Fn(usize) -> *const c_void) -> *mut SAFEARRAY {
    let len = u32::try_from(n).unwrap_or(0);
    // SAFETY: plain allocation; ownership passes to the caller (UIA).
    let sa = unsafe { SafeArrayCreateVector(vt, 0, len) };
    if sa.is_null() {
        return sa;
    }
    for i in 0..n {
        let idx = i32::try_from(i).unwrap_or(0);
        // SAFETY: index within bounds; element pointer valid for the call (copied / AddRef'd).
        let _ = unsafe { SafeArrayPutElement(sa, &idx, put(i)) };
    }
    sa
}

fn runtime_id(key: u64) -> *mut SAFEARRAY {
    let v: [i32; 3] = [
        ua::UiaAppendRuntimeId as i32,
        (key & 0xFFFF_FFFF) as u32 as i32,
        (key >> 32) as u32 as i32,
    ];
    array(VT_I4, 3, |i| {
        v.get(i)
            .map_or(std::ptr::null(), |x| std::ptr::from_ref(x).cast())
    })
}

const OPTIONS: ua::ProviderOptions = ua::ProviderOptions(
    ua::ProviderOptions_ServerSideProvider.0 | ua::ProviderOptions_UseComThreading.0,
);

fn elem(hwnd: HWND, key: u64) -> IRawElementProviderFragment {
    Elem { hwnd, key }.into()
}

fn root(hwnd: HWND) -> IRawElementProviderFragment {
    Root { hwnd }.into()
}

/// The fragment of node `i` (0 = the root).
fn frag(hwnd: HWND, t: &Tree, i: usize) -> IRawElementProviderFragment {
    match t.nodes.get(i) {
        Some(n) if i > 0 => elem(hwnd, n.key),
        _ => root(hwnd),
    }
}

fn navigate(
    hwnd: HWND,
    t: &Tree,
    i: usize,
    dir: ua::NavigateDirection,
) -> windows::core::Result<IRawElementProviderFragment> {
    let kids = |p: usize| t.children(p).collect::<Vec<_>>();
    let pick = match dir {
        ua::NavigateDirection_Parent => {
            if i == 0 {
                return Err(none());
            }
            return Ok(frag(hwnd, t, t.nodes.get(i).map_or(0, |n| n.parent)));
        }
        ua::NavigateDirection_FirstChild => kids(i).first().copied(),
        ua::NavigateDirection_LastChild => kids(i).last().copied(),
        ua::NavigateDirection_NextSibling | ua::NavigateDirection_PreviousSibling if i > 0 => {
            let sib = kids(t.nodes.get(i).map_or(0, |n| n.parent));
            let at = sib.iter().position(|&j| j == i);
            match (dir, at) {
                (ua::NavigateDirection_NextSibling, Some(p)) => sib.get(p + 1).copied(),
                (_, Some(p)) if p > 0 => sib.get(p - 1).copied(),
                _ => None,
            }
        }
        _ => None,
    };
    pick.map(|j| frag(hwnd, t, j)).ok_or_else(none)
}

/// The window: hosted by the HWND provider; fragment root of every element.
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot
)]
struct Root {
    hwnd: HWND,
}

impl IRawElementProviderSimple_Impl for Root_Impl {
    fn ProviderOptions(&self) -> windows::core::Result<ua::ProviderOptions> {
        Ok(OPTIONS)
    }
    fn GetPatternProvider(&self, _id: ua::UIA_PATTERN_ID) -> windows::core::Result<IUnknown> {
        Err(none())
    }
    fn GetPropertyValue(&self, id: ua::UIA_PROPERTY_ID) -> windows::core::Result<VARIANT> {
        Ok(match id {
            ua::UIA_ProviderDescriptionPropertyId => v_str("YAMDV"),
            _ => VARIANT::default(),
        })
    }
    fn HostRawElementProvider(&self) -> windows::core::Result<IRawElementProviderSimple> {
        let a = loaded().ok_or_else(none)?;
        let mut p = std::ptr::null_mut();
        // SAFETY: valid hwnd and out-pointer.
        unsafe { (a.host)(self.hwnd, &mut p) }.ok()?;
        if p.is_null() {
            return Err(none());
        }
        // SAFETY: an owned provider pointer.
        Ok(unsafe { IRawElementProviderSimple::from_raw(p) })
    }
}

impl IRawElementProviderFragment_Impl for Root_Impl {
    fn Navigate(
        &self,
        dir: ua::NavigateDirection,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        let t = with(self.hwnd, Win::uia_tree).ok_or_else(gone)?;
        navigate(self.hwnd, &t, 0, dir)
    }
    fn GetRuntimeId(&self) -> windows::core::Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn BoundingRectangle(&self) -> windows::core::Result<UiaRect> {
        Ok(UiaRect::default())
    }
    fn GetEmbeddedFragmentRoots(&self) -> windows::core::Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn FragmentRoot(&self) -> windows::core::Result<IRawElementProviderFragmentRoot> {
        Ok(Root { hwnd: self.hwnd }.into())
    }
}

impl IRawElementProviderFragmentRoot_Impl for Root_Impl {
    fn ElementProviderFromPoint(
        &self,
        x: f64,
        y: f64,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        let hwnd = self.hwnd;
        let hit = with(hwnd, |w, sh| {
            let t = w.uia_tree(sh);
            t.nodes
                .iter()
                .skip(1)
                .rev()
                .find(|n| {
                    let r = w.uia_screen(n.at);
                    x >= r.left && y >= r.top && x < r.left + r.width && y < r.top + r.height
                })
                .map(|n| n.key)
        })
        .ok_or_else(gone)?;
        hit.map(|k| elem(hwnd, k)).ok_or_else(none)
    }
    fn GetFocus(&self) -> windows::core::Result<IRawElementProviderFragment> {
        let hwnd = self.hwnd;
        let k = with(hwnd, |w, sh| {
            w.uia_tree(sh).nodes.iter().find(|n| n.focus).map(|n| n.key)
        })
        .flatten();
        k.map(|k| elem(hwnd, k)).ok_or_else(none)
    }
}

/// Any element below the window, found by key in a fresh snapshot.
#[implement(IRawElementProviderSimple, IRawElementProviderFragment)]
struct Elem {
    hwnd: HWND,
    key: u64,
}

impl Elem_Impl {
    fn node<T>(&self, f: impl FnOnce(&Win, &Tree, usize) -> T) -> windows::core::Result<T> {
        with(self.hwnd, |w, sh| {
            let t = w.uia_tree(sh);
            t.find(self.key).map(|i| f(w, &t, i))
        })
        .flatten()
        .ok_or_else(gone)
    }
}

impl IRawElementProviderSimple_Impl for Elem_Impl {
    fn ProviderOptions(&self) -> windows::core::Result<ua::ProviderOptions> {
        Ok(OPTIONS)
    }
    fn GetPatternProvider(&self, id: ua::UIA_PATTERN_ID) -> windows::core::Result<IUnknown> {
        let n = self
            .node(|_, t, i| t.nodes.get(i).cloned())?
            .ok_or_else(gone)?;
        let kind = Some(n.kind);
        let has = match id {
            ua::UIA_InvokePatternId => n.pat == Pat::Invoke,
            ua::UIA_TogglePatternId => matches!(n.pat, Pat::Toggle(_)),
            ua::UIA_SelectionItemPatternId => matches!(n.pat, Pat::Select(_)),
            ua::UIA_ExpandCollapsePatternId => matches!(n.pat, Pat::Expand(_)),
            ua::UIA_RangeValuePatternId => matches!(n.pat, Pat::Range(..)),
            ua::UIA_ValuePatternId => n.value.is_some(),
            _ => false,
        };
        if has {
            let p: ua::IInvokeProvider = Pats {
                hwnd: self.hwnd,
                key: self.key,
            }
            .into();
            return p.cast();
        }
        if id == ua::UIA_TextPatternId && kind == Some(K::Document) {
            let p: ITextProvider = TextProv {
                hwnd: self.hwnd,
                doc: self.key,
            }
            .into();
            return p.cast();
        }
        Err(none())
    }
    fn GetPropertyValue(&self, id: ua::UIA_PROPERTY_ID) -> windows::core::Result<VARIANT> {
        self.node(|_, t, i| {
            let Some(n) = t.nodes.get(i) else {
                return VARIANT::default();
            };
            match id {
                ua::UIA_ControlTypePropertyId => v_i4(n.kind.control_type()),
                ua::UIA_NamePropertyId => v_str(&n.name),
                ua::UIA_AutomationIdPropertyId => v_str(&format!("{:?}-{:x}", n.kind, n.key)),
                ua::UIA_IsEnabledPropertyId | ua::UIA_IsControlElementPropertyId => v_bool(true),
                ua::UIA_IsContentElementPropertyId => {
                    v_bool(!matches!(n.kind, K::Pane | K::TitleBar))
                }
                ua::UIA_IsKeyboardFocusablePropertyId => v_bool(n.kind.focusable()),
                ua::UIA_HasKeyboardFocusPropertyId => v_bool(n.focus),
                ua::UIA_IsOffscreenPropertyId => v_bool(false),
                ua::UIA_ProviderDescriptionPropertyId => v_str("YAMDV"),
                _ => VARIANT::default(),
            }
        })
    }
    fn HostRawElementProvider(&self) -> windows::core::Result<IRawElementProviderSimple> {
        Err(none())
    }
}

impl IRawElementProviderFragment_Impl for Elem_Impl {
    fn Navigate(
        &self,
        dir: ua::NavigateDirection,
    ) -> windows::core::Result<IRawElementProviderFragment> {
        let hwnd = self.hwnd;
        self.node(|_, t, i| navigate(hwnd, t, i, dir))?
    }
    fn GetRuntimeId(&self) -> windows::core::Result<*mut SAFEARRAY> {
        Ok(runtime_id(self.key))
    }
    fn BoundingRectangle(&self) -> windows::core::Result<UiaRect> {
        self.node(|w, t, i| {
            t.nodes
                .get(i)
                .map(|n| w.uia_screen(n.at))
                .unwrap_or_default()
        })
    }
    fn GetEmbeddedFragmentRoots(&self) -> windows::core::Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn FragmentRoot(&self) -> windows::core::Result<IRawElementProviderFragmentRoot> {
        Ok(Root { hwnd: self.hwnd }.into())
    }
}

/// Snapshot of element `key` (F-42 pattern getters).
fn node_of(hwnd: HWND, key: u64) -> windows::core::Result<Node> {
    with(hwnd, |w, sh| {
        let t = w.uia_tree(sh);
        t.find(key).and_then(|i| t.nodes.get(i).cloned())
    })
    .flatten()
    .ok_or_else(gone)
}

fn post(hwnd: HWND, msg: u32, wp: usize, lp: isize) -> windows::core::Result<()> {
    // SAFETY: plain message post to a window of this thread.
    unsafe {
        windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            Some(hwnd),
            msg,
            WPARAM(wp),
            LPARAM(lp),
        )
    }
}

/// Posts a click at fraction `fx` (left to right) of element `key`'s hit rect, to the window
/// or popup that shows it (F-42): the normal mouse path runs it, outside any borrow.
fn click(hwnd: HWND, key: u64, fx: f64) -> windows::core::Result<()> {
    let fx = fx.clamp(0.0, 1.0);
    let (target, x, y) = with(hwnd, |w, sh| {
        let t = w.uia_tree(sh);
        let n = t.nodes.get(t.find(key)?)?;
        match n.hit.unwrap_or(n.at) {
            At::Dip(r) => {
                let s = f64::from(w.scale);
                let x = (f64::from(r.x) + f64::from(r.w) * fx) * s;
                let y = (f64::from(r.y) + f64::from(r.h) / 2.0) * s;
                Some((hwnd, x.round() as i32, y.round() as i32))
            }
            At::Px((l, tp, r, b)) => {
                let tgt = n.target.unwrap_or(hwnd);
                let mut pt = POINT {
                    x: l + (f64::from(r - l) * fx).round() as i32,
                    y: (tp + b) / 2,
                };
                // SAFETY: valid hwnd and point.
                let _ = unsafe { windows::Win32::Graphics::Gdi::ScreenToClient(tgt, &mut pt) };
                Some((tgt, pt.x, pt.y))
            }
        }
    })
    .flatten()
    .ok_or_else(gone)?;
    let lp = (((y & 0xFFFF) as isize) << 16) | ((x & 0xFFFF) as isize);
    post(target, WM_MOUSEMOVE, 0, lp)?;
    post(target, WM_LBUTTONDOWN, 1 /* MK_LBUTTON */, lp)?;
    post(target, WM_LBUTTONUP, 0, lp)
}

/// Every control pattern of one element (F-42); `GetPatternProvider` hands it out only for the
/// patterns the element has.
#[implement(
    ua::IInvokeProvider,
    ua::IToggleProvider,
    ua::ISelectionItemProvider,
    ua::IExpandCollapseProvider,
    ua::IValueProvider,
    ua::IRangeValueProvider
)]
struct Pats {
    hwnd: HWND,
    key: u64,
}

impl Pats_Impl {
    fn n(&self) -> windows::core::Result<Node> {
        node_of(self.hwnd, self.key)
    }
    fn range(&self) -> windows::core::Result<(f64, f64, f64, f64)> {
        match self.n()?.pat {
            Pat::Range(v, lo, hi, st) => Ok((v, lo, hi, st)),
            _ => Err(E_INVALIDOP.into()),
        }
    }
    /// Expandable elements whose expansion is a popup (More menu, dropdowns, menu items) close
    /// it; outline sections toggle with another click.
    fn set_expanded(&self, open: bool) -> windows::core::Result<()> {
        let n = self.n()?;
        let Pat::Expand(now) = n.pat else {
            return Err(E_INVALIDOP.into());
        };
        if now == open {
            return Ok(());
        }
        if !open && matches!(n.kind, K::Button | K::ComboBox | K::MenuItem) {
            return post(self.hwnd, popup::WM_APP_POPUP_CLOSE, 0, 0);
        }
        click(self.hwnd, self.key, 0.5)
    }
}

impl ua::IInvokeProvider_Impl for Pats_Impl {
    fn Invoke(&self) -> windows::core::Result<()> {
        click(self.hwnd, self.key, 0.5)
    }
}

impl ua::IToggleProvider_Impl for Pats_Impl {
    fn Toggle(&self) -> windows::core::Result<()> {
        click(self.hwnd, self.key, 0.5)
    }
    fn ToggleState(&self) -> windows::core::Result<ua::ToggleState> {
        Ok(match self.n()?.pat {
            Pat::Toggle(true) => ua::ToggleState_On,
            _ => ua::ToggleState_Off,
        })
    }
}

impl ua::ISelectionItemProvider_Impl for Pats_Impl {
    fn Select(&self) -> windows::core::Result<()> {
        if self.n()?.pat == Pat::Select(true) {
            return Ok(());
        }
        click(self.hwnd, self.key, 0.5)
    }
    fn AddToSelection(&self) -> windows::core::Result<()> {
        self.Select()
    }
    fn RemoveFromSelection(&self) -> windows::core::Result<()> {
        Err(E_INVALIDOP.into())
    }
    fn IsSelected(&self) -> windows::core::Result<BOOL> {
        Ok((self.n()?.pat == Pat::Select(true)).into())
    }
    fn SelectionContainer(&self) -> windows::core::Result<IRawElementProviderSimple> {
        let hwnd = self.hwnd;
        let f = with(hwnd, |w, sh| {
            let t = w.uia_tree(sh);
            let i = t.find(self.key)?;
            Some(frag(hwnd, &t, t.nodes.get(i)?.parent))
        })
        .flatten()
        .ok_or_else(gone)?;
        f.cast()
    }
}

impl ua::IExpandCollapseProvider_Impl for Pats_Impl {
    fn Expand(&self) -> windows::core::Result<()> {
        self.set_expanded(true)
    }
    fn Collapse(&self) -> windows::core::Result<()> {
        self.set_expanded(false)
    }
    fn ExpandCollapseState(&self) -> windows::core::Result<ua::ExpandCollapseState> {
        Ok(match self.n()?.pat {
            Pat::Expand(true) => ua::ExpandCollapseState_Expanded,
            _ => ua::ExpandCollapseState_Collapsed,
        })
    }
}

impl ua::IValueProvider_Impl for Pats_Impl {
    fn SetValue(&self, val: &windows::core::PCWSTR) -> windows::core::Result<()> {
        let n = self.n()?;
        let Some((cur, e)) = n.value else {
            return Err(E_INVALIDOP.into());
        };
        if e == Edit::ReadOnly {
            return Err(E_INVALIDOP.into());
        }
        // SAFETY: UIA passes a NUL-terminated string valid for this call.
        let new = unsafe { val.as_wide() }.to_vec();
        click(self.hwnd, self.key, 0.5)?;
        const VK_END: usize = 0x23;
        const VK_BACK: usize = 0x08;
        const VK_RETURN: usize = 0x0D;
        post(self.hwnd, WM_KEYDOWN, VK_END, 1)?;
        for _ in 0..cur.encode_utf16().count() {
            post(self.hwnd, WM_KEYDOWN, VK_BACK, 1)?;
        }
        for c in new {
            post(self.hwnd, WM_CHAR, usize::from(c), 1)?;
        }
        if e == Edit::KeysEnter {
            post(self.hwnd, WM_KEYDOWN, VK_RETURN, 1)?;
        }
        Ok(())
    }
    fn Value(&self) -> windows::core::Result<BSTR> {
        Ok(BSTR::from(self.n()?.value.map(|v| v.0).unwrap_or_default()))
    }
    fn IsReadOnly(&self) -> windows::core::Result<BOOL> {
        Ok(self.n()?.value.is_none_or(|v| v.1 == Edit::ReadOnly).into())
    }
}

impl ua::IRangeValueProvider_Impl for Pats_Impl {
    fn SetValue(&self, val: f64) -> windows::core::Result<()> {
        let (_, lo, hi, _) = self.range()?;
        if !val.is_finite() || val < lo || val > hi {
            return Err(E_INVALIDARG.into());
        }
        let f = if hi > lo { (val - lo) / (hi - lo) } else { 0.0 };
        click(self.hwnd, self.key, f)
    }
    fn Value(&self) -> windows::core::Result<f64> {
        Ok(self.range()?.0)
    }
    fn IsReadOnly(&self) -> windows::core::Result<BOOL> {
        Ok(false.into())
    }
    fn Maximum(&self) -> windows::core::Result<f64> {
        Ok(self.range()?.2)
    }
    fn Minimum(&self) -> windows::core::Result<f64> {
        Ok(self.range()?.1)
    }
    fn LargeChange(&self) -> windows::core::Result<f64> {
        let (_, lo, hi, st) = self.range()?;
        Ok((st * 10.0).min(hi - lo))
    }
    fn SmallChange(&self) -> windows::core::Result<f64> {
        Ok(self.range()?.3)
    }
}

/// Read-only TextPattern of the Document element (no selection).
#[implement(ITextProvider)]
struct TextProv {
    hwnd: HWND,
    doc: u64,
}

impl TextProv_Impl {
    fn range(&self, r: Option<(usize, usize)>) -> windows::core::Result<ITextRangeProvider> {
        let text: std::rc::Rc<[u16]> = with(self.hwnd, |w, _| w.uia_text())
            .ok_or_else(gone)?
            .into();
        let r = r.unwrap_or((0, text.len()));
        Ok(TextRange {
            hwnd: self.hwnd,
            doc: self.doc,
            r: Cell::new(r),
            text,
        }
        .into())
    }
}

impl ITextProvider_Impl for TextProv_Impl {
    fn GetSelection(&self) -> windows::core::Result<*mut SAFEARRAY> {
        Ok(array(VT_UNKNOWN, 0, |_| std::ptr::null()))
    }
    fn GetVisibleRanges(&self) -> windows::core::Result<*mut SAFEARRAY> {
        let r = self.range(None)?;
        Ok(array(VT_UNKNOWN, 1, |_| r.as_raw()))
    }
    fn RangeFromChild(
        &self,
        _child: Ref<IRawElementProviderSimple>,
    ) -> windows::core::Result<ITextRangeProvider> {
        Err(E_INVALIDARG.into())
    }
    fn RangeFromPoint(&self, _pt: &UiaPoint) -> windows::core::Result<ITextRangeProvider> {
        self.range(Some((0, 0)))
    }
    fn DocumentRange(&self) -> windows::core::Result<ITextRangeProvider> {
        self.range(None)
    }
    fn SupportedTextSelection(&self) -> windows::core::Result<ua::SupportedTextSelection> {
        Ok(ua::SupportedTextSelection_None)
    }
}

/// A range of the reading text (UTF-16 offsets into a snapshot shared by clones).
#[implement(ITextRangeProvider)]
struct TextRange {
    hwnd: HWND,
    doc: u64,
    r: Cell<(usize, usize)>,
    text: std::rc::Rc<[u16]>,
}

fn other(r: Ref<ITextRangeProvider>) -> windows::core::Result<(usize, usize)> {
    let o = r.ok()?;
    // SAFETY: UIA only passes back ranges created by this provider (TextRange objects).
    let t: &TextRange = unsafe { o.as_impl() };
    Ok(t.r.get())
}

fn start(ep: ua::TextPatternRangeEndpoint) -> bool {
    ep == ua::TextPatternRangeEndpoint_Start
}

fn fold(c: u16) -> u16 {
    if (0x41..=0x5A).contains(&c) {
        c + 0x20
    } else {
        c
    }
}

impl TextRange_Impl {
    fn new_range(&self, r: (usize, usize)) -> ITextRangeProvider {
        TextRange {
            hwnd: self.hwnd,
            doc: self.doc,
            r: Cell::new(r),
            text: self.text.clone(),
        }
        .into()
    }
}

impl ITextRangeProvider_Impl for TextRange_Impl {
    fn Clone(&self) -> windows::core::Result<ITextRangeProvider> {
        Ok(self.new_range(self.r.get()))
    }
    fn Compare(&self, range: Ref<ITextRangeProvider>) -> windows::core::Result<BOOL> {
        Ok(BOOL::from(other(range)? == self.r.get()))
    }
    fn CompareEndpoints(
        &self,
        ep: ua::TextPatternRangeEndpoint,
        target: Ref<ITextRangeProvider>,
        tep: ua::TextPatternRangeEndpoint,
    ) -> windows::core::Result<i32> {
        let (a, b) = (self.r.get(), other(target)?);
        let x = if start(ep) { a.0 } else { a.1 };
        let y = if start(tep) { b.0 } else { b.1 };
        Ok(i32::try_from(x).unwrap_or(i32::MAX) - i32::try_from(y).unwrap_or(i32::MAX))
    }
    fn ExpandToEnclosingUnit(&self, unit: ua::TextUnit) -> windows::core::Result<()> {
        self.r.set(a11y::enclosing(
            &self.text,
            self.r.get().0,
            Unit::from_uia(unit.0),
        ));
        Ok(())
    }
    fn FindAttribute(
        &self,
        _id: ua::UIA_TEXTATTRIBUTE_ID,
        _val: &VARIANT,
        _back: BOOL,
    ) -> windows::core::Result<ITextRangeProvider> {
        Err(none())
    }
    fn FindText(
        &self,
        text: &BSTR,
        back: BOOL,
        icase: BOOL,
    ) -> windows::core::Result<ITextRangeProvider> {
        let needle: Vec<u16> = text.to_vec();
        let (s, e) = self.r.get();
        let hay = self.text.get(s..e).unwrap_or_default();
        if needle.is_empty() || needle.len() > hay.len() {
            return Err(none());
        }
        let eq = |w: &[u16]| {
            w.iter().zip(&needle).all(|(a, b)| {
                if icase.as_bool() {
                    fold(*a) == fold(*b)
                } else {
                    a == b
                }
            })
        };
        let mut hits = hay.windows(needle.len()).enumerate().filter(|(_, w)| eq(w));
        let hit = if back.as_bool() {
            hits.next_back()
        } else {
            hits.next()
        };
        hit.map(|(i, _)| self.new_range((s + i, s + i + needle.len())))
            .ok_or_else(none)
    }
    fn GetAttributeValue(&self, id: ua::UIA_TEXTATTRIBUTE_ID) -> windows::core::Result<VARIANT> {
        Ok(if id == ua::UIA_IsReadOnlyAttributeId {
            v_bool(true)
        } else {
            not_supported()
        })
    }
    fn GetBoundingRectangles(&self) -> windows::core::Result<*mut SAFEARRAY> {
        Ok(array(VT_R8, 0, |_| std::ptr::null()))
    }
    fn GetEnclosingElement(&self) -> windows::core::Result<IRawElementProviderSimple> {
        Ok(Elem {
            hwnd: self.hwnd,
            key: self.doc,
        }
        .into())
    }
    fn GetText(&self, max: i32) -> windows::core::Result<BSTR> {
        let (s, e) = self.r.get();
        let e = match usize::try_from(max) {
            Ok(m) => e.min(s + m),
            Err(_) => e,
        };
        Ok(BSTR::from_wide(self.text.get(s..e).unwrap_or_default()))
    }
    fn Move(&self, unit: ua::TextUnit, count: i32) -> windows::core::Result<i32> {
        let (r, n) = a11y::move_range(&self.text, self.r.get(), Unit::from_uia(unit.0), count);
        self.r.set(r);
        Ok(n)
    }
    fn MoveEndpointByUnit(
        &self,
        ep: ua::TextPatternRangeEndpoint,
        unit: ua::TextUnit,
        count: i32,
    ) -> windows::core::Result<i32> {
        let (r, n) = a11y::move_endpoint(
            &self.text,
            self.r.get(),
            start(ep),
            Unit::from_uia(unit.0),
            count,
        );
        self.r.set(r);
        Ok(n)
    }
    fn MoveEndpointByRange(
        &self,
        ep: ua::TextPatternRangeEndpoint,
        target: Ref<ITextRangeProvider>,
        tep: ua::TextPatternRangeEndpoint,
    ) -> windows::core::Result<()> {
        let o = other(target)?;
        let p = if start(tep) { o.0 } else { o.1 };
        let (s, e) = self.r.get();
        self.r.set(if start(ep) {
            (p, e.max(p))
        } else {
            (s.min(p), p)
        });
        Ok(())
    }
    fn Select(&self) -> windows::core::Result<()> {
        Err(E_INVALIDOP.into())
    }
    fn AddToSelection(&self) -> windows::core::Result<()> {
        Err(E_INVALIDOP.into())
    }
    fn RemoveFromSelection(&self) -> windows::core::Result<()> {
        Err(E_INVALIDOP.into())
    }
    fn ScrollIntoView(&self, _top: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
    fn GetChildren(&self) -> windows::core::Result<*mut SAFEARRAY> {
        Ok(array(VT_UNKNOWN, 0, |_| std::ptr::null()))
    }
}

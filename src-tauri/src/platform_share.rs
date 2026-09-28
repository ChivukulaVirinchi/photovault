//! Native file sharing for the desktop shell.

use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

/// Open the platform sharing surface. Linux has no desktop-wide share sheet,
/// so copy a standard file URI to its desktop clipboard, with the default mail
/// client as a fallback on minimal systems without a clipboard helper.
pub async fn share_file(app: AppHandle, path: PathBuf) -> Result<&'static str, String> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        let (send, receive) = tokio::sync::oneshot::channel();
        let native_app = app.clone();
        app.run_on_main_thread(move || {
            let result = share_native(&native_app, &path).map(|_| "native");
            let _ = send.send(result);
        })
        .map_err(|error| format!("could not open the share surface: {error}"))?;
        return receive
            .await
            .map_err(|_| "the share surface closed unexpectedly".to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        let _ = app;
        return tauri::async_runtime::spawn_blocking(move || share_linux(&path))
            .await
            .map_err(|error| format!("share worker failed: {error}"))?;
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = (app, path);
        Err("sharing is not supported on this platform".into())
    }
}

#[cfg(target_os = "windows")]
fn share_native(app: &AppHandle, path: &Path) -> Result<(), String> {
    use std::cell::RefCell;

    use windows::core::{AgileReference, Interface, RuntimeName, HSTRING};
    use windows::ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager};
    use windows::Foundation::TypedEventHandler;
    use windows::Storage::{IStorageItem, StorageFile};
    use windows::Win32::System::WinRT::RoGetActivationFactory;
    use windows::Win32::UI::Shell::IDataTransferManagerInterop;

    struct Registration {
        manager: DataTransferManager,
        token: i64,
    }
    thread_local! {
        // DataTransferManager is apartment-bound and this function only runs
        // on Tauri's UI thread. Keep the registration in that same apartment.
        static REGISTRATION: RefCell<Option<Registration>> = const { RefCell::new(None) };
    }

    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window is unavailable".to_string())?;
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let title = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Photo from Smriti")
        .to_string();
    // Resolve the file before Windows asks us to populate the DataPackage.
    // Blocking an async StorageFile operation inside DataRequested makes the
    // shell time out and show its generic "try that again" failure. An agile
    // reference lets the synchronous event handler use the prepared item from
    // whichever COM apartment Windows invokes it on.
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(winrt_file_path(path)))
        .and_then(|operation| operation.get())
        .map_err(|error| format!("could not prepare the file for sharing: {error}"))?;
    let item: IStorageItem = file
        .cast()
        .map_err(|error| format!("could not prepare the file for sharing: {error}"))?;
    let item = AgileReference::new(&item)
        .map_err(|error| format!("could not prepare the file for sharing: {error}"))?;

    // Desktop apps obtain DataTransferManager through its HWND interop
    // factory; GetForCurrentView is only valid for CoreWindow applications.
    let interop: IDataTransferManagerInterop =
        unsafe { RoGetActivationFactory(&HSTRING::from(DataTransferManager::NAME)) }
            .map_err(|error| format!("Windows sharing is unavailable: {error}"))?;
    let manager: DataTransferManager = unsafe { interop.GetForWindow(hwnd) }
        .map_err(|error| format!("Windows sharing is unavailable: {error}"))?;

    let handler =
        TypedEventHandler::<DataTransferManager, DataRequestedEventArgs>::new(move |_, event| {
            if let Some(event) = event.as_ref() {
                let data = event.Request()?.Data()?;
                data.Properties()?.SetTitle(&HSTRING::from(&title))?;
                let item = item.resolve()?;
                let items: windows_collections::IIterable<IStorageItem> =
                    vec![Some(item.clone())].into();
                data.SetStorageItemsReadOnly(&items)?;
            }
            Ok(())
        });
    let token = manager
        .DataRequested(&handler)
        .map_err(|error| format!("could not prepare the Windows share sheet: {error}"))?;

    REGISTRATION.with(|registration| {
        let mut registration = registration.borrow_mut();
        if let Some(previous) = registration.take() {
            let _ = previous.manager.RemoveDataRequested(previous.token);
        }
        *registration = Some(Registration {
            manager: manager.clone(),
            token,
        });
    });
    unsafe { interop.ShowShareUIForWindow(hwnd) }
        .map_err(|error| format!("could not open the Windows share sheet: {error}"))
}

/// `canonicalize` deliberately returns extended-length paths on Windows, but
/// WinRT's StorageFile parser rejects the `\\?\` prefix. Keep the canonical
/// path for containment checks and remove the prefix only at the WinRT edge.
#[cfg(target_os = "windows")]
fn winrt_file_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(dos) = path.strip_prefix(r"\\?\") {
        dos.to_string()
    } else {
        path.into_owned()
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_path_tests {
    use super::*;

    #[test]
    fn storage_file_paths_drop_extended_prefixes() {
        assert_eq!(
            winrt_file_path(Path::new(r"\\?\D:\Photos\one.jpg")),
            r"D:\Photos\one.jpg"
        );
        assert_eq!(
            winrt_file_path(Path::new(r"\\?\UNC\server\share\one.jpg")),
            r"\\server\share\one.jpg"
        );
        assert_eq!(
            winrt_file_path(Path::new(r"D:\Photos\one.jpg")),
            r"D:\Photos\one.jpg"
        );
    }
}

#[cfg(target_os = "macos")]
fn share_native(app: &AppHandle, path: &Path) -> Result<(), String> {
    use std::ffi::{c_char, c_void, CString};
    use std::sync::Mutex;

    type Id = *mut c_void;
    type Sel = *mut c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Point {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Size {
        width: f64,
        height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect {
        origin: Point,
        size: Size,
    }

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn sel_registerName(name: *const c_char) -> Sel;
        fn objc_msgSend();
    }
    #[link(name = "AppKit", kind = "framework")]
    extern "C" {}
    #[link(name = "Foundation", kind = "framework")]
    extern "C" {}

    static PICKER: Mutex<usize> = Mutex::new(0);

    unsafe fn class(name: &str) -> Id {
        let name = CString::new(name).expect("Objective-C class name");
        objc_getClass(name.as_ptr())
    }
    unsafe fn selector(name: &str) -> Sel {
        let name = CString::new(name).expect("Objective-C selector");
        sel_registerName(name.as_ptr())
    }
    unsafe fn send_id(receiver: Id, name: &str) -> Id {
        let call: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        call(receiver, selector(name))
    }
    unsafe fn send_id_arg(receiver: Id, name: &str, argument: Id) -> Id {
        let call: unsafe extern "C" fn(Id, Sel, Id) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        call(receiver, selector(name), argument)
    }
    unsafe fn send_void(receiver: Id, name: &str) {
        let call: unsafe extern "C" fn(Id, Sel) = std::mem::transmute(objc_msgSend as *const ());
        call(receiver, selector(name));
    }
    unsafe fn show_picker(picker: Id, view: Id) {
        let call: unsafe extern "C" fn(Id, Sel, Rect, Id, isize) =
            std::mem::transmute(objc_msgSend as *const ());
        call(
            picker,
            selector("showRelativeToRect:ofView:preferredEdge:"),
            Rect {
                origin: Point { x: 0.0, y: 0.0 },
                size: Size {
                    width: 0.0,
                    height: 0.0,
                },
            },
            view,
            1,
        );
    }

    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window is unavailable".to_string())?;
    let view = window.ns_view().map_err(|error| error.to_string())? as Id;
    let path = CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| "the file path contains an unsupported null byte".to_string())?;

    // SAFETY: this function only runs through `run_on_main_thread`; all
    // objects and selectors are standard AppKit/Foundation APIs. The picker is
    // retained until the next share so its popover remains alive.
    unsafe {
        let pool = send_id(send_id(class("NSAutoreleasePool"), "alloc"), "init");
        let ns_path = {
            let call: unsafe extern "C" fn(Id, Sel, *const c_char) -> Id =
                std::mem::transmute(objc_msgSend as *const ());
            call(
                class("NSString"),
                selector("stringWithUTF8String:"),
                path.as_ptr(),
            )
        };
        let url = send_id_arg(class("NSURL"), "fileURLWithPath:", ns_path);
        let items = send_id_arg(class("NSArray"), "arrayWithObject:", url);
        let picker = send_id_arg(
            send_id(class("NSSharingServicePicker"), "alloc"),
            "initWithItems:",
            items,
        );
        if picker.is_null() {
            send_void(pool, "drain");
            return Err("macOS sharing is unavailable".into());
        }

        let mut current = PICKER
            .lock()
            .map_err(|_| "macOS share state is unavailable".to_string())?;
        if *current != 0 {
            let previous = *current as Id;
            send_void(previous, "close");
            send_void(previous, "release");
        }
        show_picker(picker, view);
        *current = picker as usize;
        send_void(pool, "drain");
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn share_linux(path: &Path) -> Result<&'static str, String> {
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};

    let mut uri = String::from("file://");
    for byte in path.as_os_str().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'.' | b'_' | b'~' => {
                uri.push(*byte as char);
            }
            byte => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri.push_str("\r\n");

    for (program, args) in [
        ("wl-copy", vec!["--type", "text/uri-list"]),
        (
            "xclip",
            vec!["-selection", "clipboard", "-t", "text/uri-list"],
        ),
    ] {
        let Ok(mut child) = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(uri.as_bytes());
        }
        if child.wait().is_ok_and(|status| status.success()) {
            return Ok("clipboard");
        }
    }

    let mut command = Command::new("xdg-email");
    command
        .arg("--attach")
        .arg(path)
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: setsid is async-signal-safe and runs in the freshly forked child.
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    command
        .spawn()
        .map(|_| "email")
        .map_err(|error| format!("could not open the default mail app: {error}"))
}

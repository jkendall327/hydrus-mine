//! Qt's path-keyed panel bitmap, fixed to the visible viewport and never scaled.
#[derive(Debug, Default)]
pub struct Background {
    path: Option<String>,
    image: slint::Image,
}
impl Background {
    pub fn get(&mut self, path: Option<&str>) -> slint::Image {
        let Some(path) = path else {
            return slint::Image::default();
        };
        if self.path.as_deref() != Some(path) {
            self.path = Some(path.to_owned());
            self.image =
                slint::Image::load_from_path(std::path::Path::new(path)).unwrap_or_default();
        }
        self.image.clone()
    }
    pub fn clear(&mut self) {
        self.path = None;
        self.image = slint::Image::default();
    }
}

/// One-file chooser, seeded by the staged path. Native dialog cancellation
/// returns None; callers revalidate their Options owner before changing it.
pub fn pick(existing: &str) -> Option<String> {
    let selected = if let Some(picker) = crate::PICKER.with(|picker| picker.borrow().clone()) {
        picker(crate::Pick::Files, "").into_iter().next()
    } else {
        let mut dialog = rfd::FileDialog::new();
        let path = std::path::Path::new(existing);
        if path.is_dir() {
            dialog = dialog.set_directory(path);
        } else {
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                dialog = dialog.set_directory(parent);
            }
            if let Some(name) = path.file_name() {
                dialog = dialog.set_file_name(name.to_string_lossy());
            }
        }
        dialog.pick_file()
    }?;
    Some(normalize(&selected).to_string_lossy().into_owned())
}

fn normalize(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut output = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if output.file_name().is_some_and(|name| name != "..") {
                    output.pop();
                } else if !output.has_root() {
                    output.push("..");
                }
            }
            component => output.push(component.as_os_str()),
        }
    }
    if output.as_os_str().is_empty() {
        output.push(".");
    }
    output
}

pub fn normalized_path(path: &str) -> String {
    normalize(std::path::Path::new(path))
        .to_string_lossy()
        .into_owned()
}

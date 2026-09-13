use super::*;

pub(crate) fn build_process_scripted_ui_presentation(
    host_path: &Path,
    package_root: Option<&Path>,
) -> Result<(ScriptedUiPresentation, AddonProcessSupervisor), String> {
    let mut command = Command::new(host_path);
    command.arg("--process-host");
    if let Some(package_root) = package_root {
        command.args(["--package-root", package_root.to_string_lossy().as_ref()]);
    }
    let mut child_guard = ChildGuard(Some(
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("cannot start addon process host: {error}"))?,
    ));
    let Some(stdin) = child_guard.0.as_mut().and_then(|child| child.stdin.take()) else {
        return Err("addon process host did not expose stdin".to_owned());
    };
    let Some(stdout) = child_guard.0.as_mut().and_then(|child| child.stdout.take()) else {
        return Err("addon process host did not expose stdout".to_owned());
    };
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| format!("cannot read addon process host readiness: {error}"))?;
    if line.trim() != "READY" {
        return Err(format!(
            "addon process host readiness was {:?}",
            line.trim()
        ));
    }
    let mut stdin = stdin;
    writeln!(stdin, "render")
        .and_then(|_| stdin.flush())
        .map_err(|error| format!("cannot request addon panel: {error}"))?;
    line.clear();
    reader
        .read_line(&mut line)
        .map_err(|error| format!("cannot read addon panel: {error}"))?;
    let mut fields = line.trim().split('\t');
    if fields.next() != Some("PANEL") {
        return Err(format!("addon process host returned {:?}", line.trim()));
    }
    let node_id = fields
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value != 0)
        .ok_or_else(|| "addon process host returned an invalid node ID".to_owned())?;
    let label = fields
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "addon process host returned an empty panel label".to_owned())?
        .to_owned();
    if fields.next().is_some() {
        return Err("addon process host returned extra panel fields".to_owned());
    }
    let child = child_guard
        .0
        .take()
        .expect("addon process child guard must contain the child");
    println!(
        "SCRIPTED_UI source=wasmi-process host={} node={node_id} label={label}",
        host_path.display()
    );
    let presentation = ScriptedUiPresentation {
        default_label: label.clone(),
        addon_label: label,
        default_node_id: node_id,
        addon_node_id: node_id.saturating_add(1),
    };
    Ok((
        presentation,
        AddonProcessSupervisor(Arc::new(Mutex::new(Some(AddonProcessHandle {
            child,
            stdin,
        })))),
    ))
}

pub(crate) fn build_scripted_ui_presentation(
    addon_root: Option<&Path>,
) -> Result<ScriptedUiPresentation, String> {
    if let Some(addon_root) = addon_root {
        let repository = ui_scripting_spike::PackageRepository::new(addon_root);
        let known_capabilities = std::collections::BTreeSet::new();
        let mut runners = repository
            .load_all(AddonPolicy::default(), &known_capabilities)
            .map_err(|error| format!("cannot load addon repository: {error}"))?;
        if runners.len() < 2 {
            return Err("addon repository must contain at least two packages".to_owned());
        }
        println!(
            "SCRIPTED_UI source=repository root={}",
            addon_root.display()
        );
        let mut default_ui = runners.remove(0);
        let mut addon = runners.remove(0);
        return scripted_ui_from_runners(&mut default_ui, &mut addon);
    }
    let default_source = r#"
        local panel = ui.create_panel("Default UI secure attack")
        ui.set_position(panel, 12, 12)
    "#;
    let addon_source = r#"
        local panel = ui.create_panel("Addon secure attack presentation")
        ui.set_position(panel, 12, 42)
    "#;
    let mut default_ui = AddonRunner::load("default-ui", default_source, AddonPolicy::default())
        .expect("default UI addon must load during client startup");
    let mut addon = AddonRunner::load("starter-addon", addon_source, AddonPolicy::default())
        .expect("ordinary UI addon must load during client startup");
    scripted_ui_from_runners(&mut default_ui, &mut addon)
}

pub(crate) fn scripted_ui_from_runners(
    default_ui: &mut AddonRunner,
    addon: &mut AddonRunner,
) -> Result<ScriptedUiPresentation, String> {
    let default_node = default_ui
        .snapshot()
        .nodes
        .first()
        .ok_or_else(|| "default UI addon must describe one panel".to_owned())
        .cloned()?;
    let addon_node = addon
        .snapshot()
        .nodes
        .first()
        .ok_or_else(|| "ordinary UI addon must describe one panel".to_owned())
        .cloned()?;
    default_ui
        .secure_input(default_node.id, "basic_attack")
        .map_err(|error| format!("default UI secure action rejected: {error}"))?;
    addon
        .secure_input(addon_node.id, "basic_attack")
        .map_err(|error| format!("addon secure action rejected: {error}"))?;
    println!(
        "SCRIPTED_UI default_node={} addon_node={} action=basic_attack",
        default_node.id, addon_node.id
    );
    Ok(ScriptedUiPresentation {
        default_label: default_node.text,
        addon_label: addon_node.text,
        default_node_id: default_node.id,
        addon_node_id: addon_node.id,
    })
}

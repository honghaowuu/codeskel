use crate::cli::PomArgs;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
pub struct PomOutput {
    pub service_name: String,
    pub group_id: String,
    pub version: String,
    pub pom_path: String,
    pub is_multi_module: bool,
    pub internal_sdk_deps: Vec<SdkDep>,
    pub existing_skill_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SdkDep {
    pub artifact_id: String,
    pub group_id: String,
    pub version: String,
}

/// Get direct child element's text content.
fn xml_text<'a, 'i>(node: &roxmltree::Node<'a, 'i>, tag: &str) -> Option<String> {
    node.children()
        .into_iter()
        .filter(|n| n.is_element() && n.tag_name().name() == tag)
        .find_map(|n| n.text().map(|s| s.trim().to_owned()))
}

/// Get direct child element node.
fn xml_child<'a, 'i>(node: &roxmltree::Node<'a, 'i>, tag: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.children()
        .into_iter()
        .filter(|n| n.is_element() && n.tag_name().name() == tag)
        .next()
}

/// Resolve a `${prop}` reference from module then parent properties.
fn resolve_property<'a, 'i>(
    value: &str,
    module: &roxmltree::Node<'a, 'i>,
    parent: Option<&roxmltree::Node<'a, 'i>>,
) -> String {
    if let Some(rest) = value.strip_prefix("${") {
        if let Some(prop_name) = rest.strip_suffix("}") {
            // Try module properties first
            if let Some(props) = xml_child(module, "properties") {
                if let Some(val) = xml_text(&props, prop_name) {
                    return resolve_property(&val, module, parent);
                }
            }
            // Try parent properties
            if let Some(parent_node) = parent {
                if let Some(props) = xml_child(parent_node, "properties") {
                    if let Some(val) = xml_text(&props, prop_name) {
                        return resolve_property(&val, module, parent);
                    }
                }
            }
            // Return unresolved (keep raw `${...}`)
            return value.to_owned();
        }
    }
    value.to_owned()
}

/// Extract internal SDK dependencies from a POM node.
fn extract_sdk_deps<'a, 'i>(
    module: &roxmltree::Node<'a, 'i>,
    parent: Option<&roxmltree::Node<'a, 'i>>,
    root_group_id: &str,
) -> Vec<SdkDep> {
    let mut deps = Vec::new();
    let dep_nodes: Vec<roxmltree::Node> = xml_child(module, "dependencies")
        .map(|deps_node| {
            deps_node
                .children()
                .into_iter()
                .filter(|n| n.is_element() && n.tag_name().name() == "dependency")
                .collect()
        })
        .unwrap_or_default();

    // Also collect from parent
    let parent_deps: Vec<roxmltree::Node> = parent
        .and_then(|p| xml_child(p, "dependencies"))
        .map(|deps_node| {
            deps_node
                .children()
                .into_iter()
                .filter(|n| n.is_element() && n.tag_name().name() == "dependency")
                .collect()
        })
        .unwrap_or_default();

    for dep_node in dep_nodes.into_iter().chain(parent_deps.into_iter()) {
        let artifact_id = match xml_text(&dep_node, "artifactId") {
            Some(a) => a,
            None => continue,
        };
        let group_id = match xml_text(&dep_node, "groupId") {
            Some(g) => g,
            // Skip dependencies without groupId
            None => continue,
        };
        let version = match xml_text(&dep_node, "version") {
            Some(v) => resolve_property(&v, module, parent),
            None => "".to_owned(),
        };

        // Filter: artifactId ends with `-api` or `-sdk` AND groupId starts with root groupId prefix
        if (artifact_id.ends_with("-api") || artifact_id.ends_with("-sdk"))
            && group_id.starts_with(root_group_id)
        {
            deps.push(SdkDep {
                artifact_id,
                group_id,
                version,
            });
        }
    }
    deps
}

/// Main extraction function.
fn extract_output(
    pom_path: &Path,
    content: &str,
    parent_content: Option<&str>,
    is_multi_module: bool,
) -> anyhow::Result<PomOutput> {
    let doc = roxmltree::Document::parse(content)?;

    // Also parse parent if provided (borrows from parent_content string)
    let parent_doc: Option<roxmltree::Document> = parent_content.map(|c| roxmltree::Document::parse(c)).transpose()?;
    let parent_node: Option<roxmltree::Node> = parent_doc.as_ref().map(|d| d.root().first_child()).flatten();

    // doc.root() is the Document/Root node; the actual root element is first_child
    let root_node = doc.root().first_child().ok_or_else(|| anyhow::anyhow!("No root element in pom.xml"))?;

    // Extract artifactId (required)
    let artifact_id = xml_text(&root_node, "artifactId")
        .ok_or_else(|| anyhow::anyhow!("<artifactId> not found in pom.xml"))?;

    // Extract groupId: module first, then parent, then fail
    let group_id = xml_text(&root_node, "groupId")
        .or_else(|| parent_node.as_ref().and_then(|p| xml_text(p, "groupId")))
        .ok_or_else(|| anyhow::anyhow!("<groupId> not found in pom.xml or parent"))?;

    // Extract version: module first, then parent, then default
    let version = xml_text(&root_node, "version")
        .or_else(|| parent_node.as_ref().and_then(|p| xml_text(p, "version")))
        .unwrap_or_else(|| "0.0.1-SNAPSHOT".to_owned());
    let resolved_version = resolve_property(&version, &root_node, parent_node.as_ref());

    // Extract internal SDK deps
    let internal_sdk_deps = extract_sdk_deps(&root_node, parent_node.as_ref(), &group_id);

    // Check for existing skill path
    let pom_dir = pom_path.parent().unwrap_or(Path::new("."));
    let skill_path = pom_dir.join("docs/skills").join(&artifact_id).join("SKILL.md");
    let existing_skill_path = if skill_path.exists() {
        Some(skill_path.to_string_lossy().to_string())
    } else {
        None
    };

    Ok(PomOutput {
        service_name: artifact_id,
        group_id,
        version: resolved_version,
        pom_path: pom_path.to_string_lossy().to_string(),
        is_multi_module,
        internal_sdk_deps,
        existing_skill_path,
    })
}

pub fn run(args: PomArgs) -> anyhow::Result<bool> {
    let project_root = args.project_root;
    if !project_root.is_dir() {
        return Err(crate::error::CodeskelError::ProjectRootMissing(project_root).into());
    }
    let pom_path = project_root.join("pom.xml");

    if !pom_path.exists() {
        anyhow::bail!("pom.xml not found at {}", pom_path.display());
    }

    let content = std::fs::read_to_string(&pom_path)
        .map_err(|e| anyhow::anyhow!("Failed to read pom.xml: {}", e))?;

    let root_doc = roxmltree::Document::parse(&content)?;
    let root_element = root_doc.root().first_child()
        .ok_or_else(|| anyhow::anyhow!("No root element in pom.xml"))?;

    // Check if multi-module: does root have <modules> child element?
    let is_multi_module = xml_child(&root_element, "modules").is_some();

    if is_multi_module {
        // Require --controller-path for multi-module
        let controller_path = args.controller_path
            .ok_or_else(|| anyhow::anyhow!("Multi-module project detected: --controller-path is required"))?;

        // Find the sub-module whose directory contains controller_path
        let modules_node = xml_child(&root_element, "modules")
            .expect("modules node already confirmed to exist");

        let module_elements: Vec<roxmltree::Node> = modules_node
            .children()
            .into_iter()
            .filter(|n| n.is_element() && n.tag_name().name() == "module")
            .collect();

        let mut found_module = false;
        let mut module_content: Option<String> = None;
        let mut module_path: Option<PathBuf> = None;

        for module_elem in module_elements {
            let module_name = module_elem.text().map(|s| s.trim()).unwrap_or("");
            if module_name.is_empty() {
                continue;
            }
            let module_dir = project_root.join(module_name);
            let module_pom = module_dir.join("pom.xml");
            if module_pom.exists() {
                // Check if controller_path exists in this module
                let ctrl = module_dir.join(&controller_path);
                if ctrl.exists() {
                    let mc = std::fs::read_to_string(&module_pom)
                        .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", module_pom.display(), e))?;
                    found_module = true;
                    module_content = Some(mc);
                    module_path = Some(module_pom);
                    break;
                }
            }
        }

        if !found_module {
            anyhow::bail!(
                "Could not find module containing '{}' in multi-module project",
                controller_path
            );
        }

        // Parse parent POM for property/dep resolution
        let parent_content: Option<String> = Some(content);

        let output = extract_output(
            &module_path.unwrap(),
            module_content.as_deref().unwrap(),
            parent_content.as_deref(),
            true,
        )?;

        println!("{}", crate::envelope::format_ok(serde_json::to_value(&output)?));
        return Ok(false);
    } else {
        let output = extract_output(&pom_path, &content, None, false)?;
        println!("{}", crate::envelope::format_ok(serde_json::to_value(&output)?));
        return Ok(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_POM: &str = r#"<?xml version="1.0"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>billing-service</artifactId>
  <version>1.2.0</version>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>user-api</artifactId>
      <version>2.1.0</version>
    </dependency>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>core-lib</artifactId>
      <version>1.0.0</version>
    </dependency>
    <dependency>
      <groupId>org.springframework</groupId>
      <artifactId>spring-web-api</artifactId>
      <version>5.0</version>
    </dependency>
  </dependencies>
</project>"#;

    const PROP_POM: &str = r#"<?xml version="1.0"?>
<project>
  <groupId>com.example</groupId>
  <artifactId>billing-service</artifactId>
  <version>${billing.version}</version>
  <properties>
    <billing.version>3.0.0</billing.version>
  </properties>
</project>"#;

    #[test]
    fn test_simple_pom_extraction() {
        let tmp = tempfile::tempdir().unwrap();
        let pom_path = tmp.path().join("pom.xml");
        std::fs::write(&pom_path, SIMPLE_POM).unwrap();
        let output = extract_output(&pom_path, SIMPLE_POM, None, false).unwrap();
        assert_eq!(output.service_name, "billing-service");
        assert_eq!(output.group_id, "com.example");
        assert_eq!(output.version, "1.2.0");
        assert!(!output.is_multi_module);
    }

    #[test]
    fn test_sdk_dep_filtering() {
        let tmp = tempfile::tempdir().unwrap();
        let pom_path = tmp.path().join("pom.xml");
        std::fs::write(&pom_path, SIMPLE_POM).unwrap();
        let output = extract_output(&pom_path, SIMPLE_POM, None, false).unwrap();
        assert_eq!(output.internal_sdk_deps.len(), 1);
        assert_eq!(output.internal_sdk_deps[0].artifact_id, "user-api");
        // core-lib: not -api/-sdk suffix → excluded
        // spring-web-api: not com.example groupId → excluded
    }

    #[test]
    fn test_property_version_resolution() {
        let tmp = tempfile::tempdir().unwrap();
        let pom_path = tmp.path().join("pom.xml");
        std::fs::write(&pom_path, PROP_POM).unwrap();
        let output = extract_output(&pom_path, PROP_POM, None, false).unwrap();
        assert_eq!(output.version, "3.0.0");
    }

    #[test]
    fn test_existing_skill_path_null() {
        let tmp = tempfile::tempdir().unwrap();
        let pom_path = tmp.path().join("pom.xml");
        std::fs::write(&pom_path, SIMPLE_POM).unwrap();
        let output = extract_output(&pom_path, SIMPLE_POM, None, false).unwrap();
        assert!(output.existing_skill_path.is_none());
    }

    #[test]
    fn test_existing_skill_path_found() {
        let tmp = tempfile::tempdir().unwrap();
        let pom_path = tmp.path().join("pom.xml");
        std::fs::write(&pom_path, SIMPLE_POM).unwrap();
        let skill_dir = tmp.path().join("docs/skills/billing-service");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# skill").unwrap();
        let output = extract_output(&pom_path, SIMPLE_POM, None, false).unwrap();
        assert!(output.existing_skill_path.is_some());
        assert!(output.existing_skill_path.unwrap().ends_with("SKILL.md"));
    }
}

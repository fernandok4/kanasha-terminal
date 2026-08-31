use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Area {
    pub id: String,
    pub name: String,
    pub root_path: String,
    pub workspaces: Vec<Workspace>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub panels: Vec<TerminalPanel>,
    pub layout: Option<LayoutNode>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPanel {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub profile_id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LayoutNode {
    Panel {
        #[serde(rename = "panelId", alias = "panel_id")]
        panel_id: String,
    },
    Split {
        #[serde(default = "new_id")]
        id: String,
        direction: SplitDirection,
        #[serde(default = "default_split_ratio")]
        ratio: f64,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

fn default_split_ratio() -> f64 {
    0.5
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ReorderDirection {
    Earlier,
    Later,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedProfile {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PersistedState {
    #[serde(default = "default_state_version")]
    pub version: u8,
    #[serde(default)]
    pub areas: Vec<Area>,
    #[serde(default)]
    pub custom_profiles: Vec<PersistedProfile>,
}

fn default_state_version() -> u8 {
    2
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    pub areas: Vec<Area>,
    pub profiles: Vec<ProfileView>,
    pub active_terminal_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub built_in: bool,
    pub available: bool,
    pub configured: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAreaInput {
    pub name: String,
    pub root_path: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceInput {
    pub area_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTerminalInput {
    pub workspace_id: String,
    pub profile_id: String,
    pub title: Option<String>,
    pub target_panel_id: Option<String>,
    pub direction: Option<SplitDirection>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigureProfileInput {
    pub profile_id: String,
    pub executable: String,
    pub arguments: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResizeTerminalInput {
    pub terminal_id: String,
    pub columns: u16,
    pub rows: u16,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSplitRatioInput {
    pub workspace_id: String,
    pub split_id: String,
    pub ratio: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetTerminalLabelInput {
    pub terminal_id: String,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutput {
    pub terminal_id: String,
    pub data: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExit {
    pub terminal_id: String,
}

impl Workspace {
    pub fn add_panel(
        &mut self,
        panel: TerminalPanel,
        target_panel_id: Option<&str>,
        direction: SplitDirection,
    ) -> Result<(), String> {
        let new_panel_id = panel.id.clone();
        self.panels.push(panel);

        let next_layout = match self.layout.take() {
            None => Some(LayoutNode::Panel {
                panel_id: new_panel_id,
            }),
            Some(mut layout) => {
                let target = target_panel_id
                    .map(ToOwned::to_owned)
                    .or_else(|| layout.first_panel_id().map(ToOwned::to_owned))
                    .ok_or_else(|| "O layout não contém painel para dividir.".to_string())?;

                if !layout.split_at(&target, &new_panel_id, direction) {
                    self.panels.retain(|item| item.id != new_panel_id);
                    self.layout = Some(layout);
                    return Err("O painel escolhido não pertence a este workspace.".to_string());
                }
                Some(layout)
            }
        };

        self.layout = next_layout;
        Ok(())
    }

    pub fn remove_panel(&mut self, panel_id: &str) -> bool {
        let original_len = self.panels.len();
        self.panels.retain(|panel| panel.id != panel_id);
        if self.panels.len() == original_len {
            return false;
        }

        self.layout = self
            .layout
            .take()
            .and_then(|layout| layout.without_panel(panel_id));
        true
    }

    pub fn panel(&self, panel_id: &str) -> Option<&TerminalPanel> {
        self.panels.iter().find(|panel| panel.id == panel_id)
    }

    pub fn panel_mut(&mut self, panel_id: &str) -> Option<&mut TerminalPanel> {
        self.panels.iter_mut().find(|panel| panel.id == panel_id)
    }

    pub fn panel_ids(&self) -> impl Iterator<Item = &str> {
        self.panels.iter().map(|panel| panel.id.as_str())
    }

    pub fn clone_for_duplicate(&self) -> Self {
        let mut duplicate = self.clone();
        duplicate.id = new_id();
        duplicate.name = format!("{} (cópia)", self.name);

        let remapped_panels = duplicate
            .panels
            .iter()
            .map(|panel| (panel.id.clone(), new_id()))
            .collect::<Vec<_>>();

        for panel in &mut duplicate.panels {
            if let Some((_, replacement)) = remapped_panels.iter().find(|(old, _)| old == &panel.id)
            {
                panel.id = replacement.clone();
            }
        }

        if let Some(layout) = &mut duplicate.layout {
            layout.remap_panel_ids(&remapped_panels);
        }
        duplicate
    }
}

impl LayoutNode {
    pub fn first_panel_id(&self) -> Option<&str> {
        match self {
            Self::Panel { panel_id } => Some(panel_id),
            Self::Split { first, .. } => first.first_panel_id(),
        }
    }

    fn split_at(
        &mut self,
        target_panel_id: &str,
        new_panel_id: &str,
        direction: SplitDirection,
    ) -> bool {
        match self {
            Self::Panel { panel_id } if panel_id == target_panel_id => {
                let original = self.clone();
                *self = Self::Split {
                    id: new_id(),
                    direction,
                    ratio: default_split_ratio(),
                    first: Box::new(original),
                    second: Box::new(Self::Panel {
                        panel_id: new_panel_id.to_string(),
                    }),
                };
                true
            }
            Self::Panel { .. } => false,
            Self::Split { first, second, .. } => {
                first.split_at(target_panel_id, new_panel_id, direction.clone())
                    || second.split_at(target_panel_id, new_panel_id, direction)
            }
        }
    }

    pub fn set_split_ratio(&mut self, split_id: &str, ratio: f64) -> bool {
        match self {
            Self::Panel { .. } => false,
            Self::Split {
                id,
                ratio: current_ratio,
                first,
                second,
                ..
            } if id == split_id => {
                *current_ratio = ratio;
                true
            }
            Self::Split { first, second, .. } => {
                first.set_split_ratio(split_id, ratio) || second.set_split_ratio(split_id, ratio)
            }
        }
    }

    fn without_panel(self, panel_id: &str) -> Option<Self> {
        match self {
            Self::Panel { panel_id: current } if current == panel_id => None,
            Self::Panel { .. } => Some(self),
            Self::Split {
                id,
                direction,
                ratio,
                first,
                second,
            } => match (
                first.without_panel(panel_id),
                second.without_panel(panel_id),
            ) {
                (None, None) => None,
                (Some(remaining), None) | (None, Some(remaining)) => Some(remaining),
                (Some(first), Some(second)) => Some(Self::Split {
                    id,
                    direction,
                    ratio,
                    first: Box::new(first),
                    second: Box::new(second),
                }),
            },
        }
    }

    fn remap_panel_ids(&mut self, mappings: &[(String, String)]) {
        match self {
            Self::Panel { panel_id } => {
                if let Some((_, replacement)) = mappings.iter().find(|(old, _)| old == panel_id) {
                    *panel_id = replacement.clone();
                }
            }
            Self::Split { first, second, .. } => {
                first.remap_panel_ids(mappings);
                second.remap_panel_ids(mappings);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panel(id: &str) -> TerminalPanel {
        TerminalPanel {
            id: id.to_string(),
            title: id.to_string(),
            label: None,
            profile_id: "shell".to_string(),
        }
    }

    #[test]
    fn split_and_remove_panels_preserves_a_valid_layout() {
        let mut workspace = Workspace {
            id: new_id(),
            name: "Teste".to_string(),
            panels: Vec::new(),
            layout: None,
        };

        workspace
            .add_panel(panel("first"), None, SplitDirection::Vertical)
            .expect("first panel should be added");
        workspace
            .add_panel(panel("second"), Some("first"), SplitDirection::Horizontal)
            .expect("second panel should split the first");

        assert!(workspace.remove_panel("first"));
        assert_eq!(workspace.panels, vec![panel("second")]);
        assert_eq!(
            workspace.layout,
            Some(LayoutNode::Panel {
                panel_id: "second".to_string()
            })
        );
    }

    #[test]
    fn removing_one_of_four_panels_collapses_only_its_empty_split() {
        let mut workspace = Workspace {
            id: new_id(),
            name: "Teste".to_string(),
            panels: Vec::new(),
            layout: None,
        };

        workspace
            .add_panel(panel("first"), None, SplitDirection::Vertical)
            .expect("first panel should be added");
        workspace
            .add_panel(panel("second"), Some("first"), SplitDirection::Horizontal)
            .expect("second panel should split the first");
        workspace
            .add_panel(panel("third"), Some("second"), SplitDirection::Vertical)
            .expect("third panel should split the second");
        workspace
            .add_panel(panel("fourth"), Some("third"), SplitDirection::Horizontal)
            .expect("fourth panel should split the third");

        assert!(workspace.remove_panel("fourth"));
        assert_eq!(
            workspace.panel_ids().collect::<Vec<_>>(),
            vec!["first", "second", "third"]
        );
        match workspace.layout.expect("layout should remain") {
            LayoutNode::Split {
                direction: SplitDirection::Horizontal,
                first,
                second,
                ..
            } => {
                assert_eq!(
                    *first,
                    LayoutNode::Panel {
                        panel_id: "first".to_string(),
                    }
                );
                match *second {
                    LayoutNode::Split {
                        direction: SplitDirection::Vertical,
                        first,
                        second,
                        ..
                    } => {
                        assert_eq!(
                            *first,
                            LayoutNode::Panel {
                                panel_id: "second".to_string(),
                            }
                        );
                        assert_eq!(
                            *second,
                            LayoutNode::Panel {
                                panel_id: "third".to_string(),
                            }
                        );
                    }
                    _ => panic!("remaining nested split should remain"),
                }
            }
            _ => panic!("only the empty split should be collapsed"),
        }
    }

    #[test]
    fn duplicated_workspace_uses_new_workspace_and_panel_ids() {
        let original = Workspace {
            id: "workspace-original".to_string(),
            name: "Planejamento".to_string(),
            panels: vec![panel("terminal-original")],
            layout: Some(LayoutNode::Panel {
                panel_id: "terminal-original".to_string(),
            }),
        };

        let duplicate = original.clone_for_duplicate();

        assert_ne!(duplicate.id, original.id);
        assert_ne!(duplicate.panels[0].id, original.panels[0].id);
        assert_eq!(
            duplicate.layout.unwrap().first_panel_id(),
            Some(duplicate.panels[0].id.as_str())
        );
    }

    #[test]
    fn layout_uses_camel_case_panel_id_and_reads_legacy_state() {
        let layout = LayoutNode::Panel {
            panel_id: "panel-1".into(),
        };
        let serialized = serde_json::to_value(&layout).expect("serialize layout");
        assert_eq!(serialized["panelId"], "panel-1");

        let legacy: LayoutNode = serde_json::from_value(serde_json::json!({
            "kind": "panel",
            "panel_id": "panel-legado"
        }))
        .expect("read legacy layout");
        assert_eq!(legacy.first_panel_id(), Some("panel-legado"));
    }

    #[test]
    fn split_ratio_is_persisted_and_legacy_split_gets_safe_defaults() {
        let mut workspace = Workspace {
            id: new_id(),
            name: "Teste".to_string(),
            panels: vec![panel("first"), panel("second")],
            layout: Some(LayoutNode::Split {
                id: "split-1".to_string(),
                direction: SplitDirection::Vertical,
                ratio: 0.5,
                first: Box::new(LayoutNode::Panel {
                    panel_id: "first".to_string(),
                }),
                second: Box::new(LayoutNode::Panel {
                    panel_id: "second".to_string(),
                }),
            }),
        };

        assert!(workspace
            .layout
            .as_mut()
            .expect("layout")
            .set_split_ratio("split-1", 0.72));
        let serialized = serde_json::to_value(&workspace).expect("serialize workspace");
        assert_eq!(serialized["layout"]["ratio"], 0.72);
        assert_eq!(serialized["layout"]["id"], "split-1");

        let legacy: LayoutNode = serde_json::from_value(serde_json::json!({
            "kind": "split",
            "direction": "vertical",
            "first": { "kind": "panel", "panelId": "first" },
            "second": { "kind": "panel", "panelId": "second" }
        }))
        .expect("read legacy split");
        match legacy {
            LayoutNode::Split { id, ratio, .. } => {
                assert!(!id.is_empty());
                assert_eq!(ratio, 0.5);
            }
            LayoutNode::Panel { .. } => panic!("expected split"),
        }
    }
}

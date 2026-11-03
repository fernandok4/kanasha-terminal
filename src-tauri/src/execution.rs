//! Per-person Codex settings. These are execution parameters, never prompt hints.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const SOL: &str = "gpt-6-sol";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentExecution {
    pub level: String,
    pub model: String,
    pub reasoning_effort: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_preserved_and_manager_can_choose_catalog_models() {
        for (level, model, effort) in [
            ("junior", "gpt-6-luna", "xhigh"),
            ("mid", "gpt-5.6-terra", "xhigh"),
            ("senior", SOL, "xhigh"),
            ("specialist", "gpt-6-astra", "high"),
            ("manager", SOL, "xhigh"),
        ] {
            let config = AgentExecution::preset(level).unwrap();
            assert_eq!((&*config.model, &*config.reasoning_effort), (model, effort));
            config.validate(level == "manager").unwrap();
        }
        let mut config = AgentExecution::manager();
        config.model = "gpt-6-astra".into();
        config.validate(true).unwrap();
        config.level = "junior".into();
        assert!(config.validate(false).is_err());
        config.level = "custom".into();
        config.validate(false).unwrap();
    }

    #[test]
    fn unavailable_settings_and_silent_substitution_are_rejected() {
        let config = AgentExecution::preset("junior").unwrap();
        assert!(config.verify_catalog(&[]).is_err());
        assert!(config.verify_catalog(&[json!({"model":"gpt-6-luna","supportedReasoningEfforts":[{"reasoningEffort":"high"}]})]).is_err());
        config.verify_catalog(&[json!({"model":"gpt-6-luna","supportedReasoningEfforts":[{"reasoningEffort":"xhigh"}]})]).unwrap();
        assert!(config
            .verify_response(&json!({"model":SOL,"reasoningEffort":"xhigh"}))
            .is_err());
        assert!(config
            .verify_response(&json!({"model":"gpt-6-luna","reasoningEffort":"medium"}))
            .is_err());
        let params = config.turn_params("thread", "Objetivo");
        assert_eq!(params["model"], "gpt-6-luna");
        assert_eq!(params["effort"], "xhigh");
    }

    #[test]
    fn new_models_and_efforts_require_catalog_confirmation() {
        for model in ["gpt-6.1-sol", "gpt-5.6-sol", "gpt-5.6-luna", "future-model"] {
            let config = AgentExecution {
                level: "custom".into(),
                model: model.into(),
                reasoning_effort: "ultra".into(),
            };
            config.validate(false).unwrap();
            assert!(config.verify_catalog(&[]).is_err());
            config.verify_catalog(&[json!({"model":model,"supportedReasoningEfforts":[{"reasoningEffort":"ultra"}]})]).unwrap();
            config
                .verify_response(&json!({"model":model,"reasoningEffort":"ultra"}))
                .unwrap();
            assert!(config
                .verify_response(&json!({"model":SOL,"reasoningEffort":"ultra"}))
                .is_err());
        }
        for invalid in ["", "gpt\"\nfoo=true", "gpt model", "model\\escape"] {
            let config = AgentExecution {
                level: "custom".into(),
                model: invalid.into(),
                reasoning_effort: "high".into(),
            };
            assert!(config.validate(false).is_err());
        }
    }
}

impl Default for AgentExecution {
    fn default() -> Self {
        Self::preset("senior").unwrap()
    }
}

impl AgentExecution {
    pub fn manager() -> Self {
        Self::preset("manager").unwrap()
    }

    pub fn preset(level: &str) -> Result<Self, String> {
        let (model, effort) = match level {
            "junior" => ("gpt-6-luna", "xhigh"),
            "mid" => ("gpt-5.6-terra", "xhigh"),
            "senior" | "manager" => (SOL, "xhigh"),
            "specialist" => ("gpt-6-astra", "high"),
            _ => return Err("Nível profissional inválido.".into()),
        };
        Ok(Self {
            level: level.into(),
            model: model.into(),
            reasoning_effort: effort.into(),
        })
    }

    pub fn validate(&self, manager: bool) -> Result<(), String> {
        // Availability is checked against model/list before execution, not a static allowlist.
        // Restrict identifiers so persisted settings remain safe in CLI config arguments.
        let valid_identifier = |value: &str| {
            !value.is_empty()
                && value.len() <= 160
                && value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-._/:".contains(&c))
        };
        if !valid_identifier(&self.model) || !valid_identifier(&self.reasoning_effort) {
            return Err("Escolha um modelo e nível de raciocínio válidos.".into());
        }
        if manager {
            if self.level != "manager" {
                return Err("O gerente deve manter o nível Gerente.".into());
            }
        } else if self.level != "custom" && Self::preset(&self.level)? != *self {
            return Err("A combinação não corresponde ao nível escolhido. Use Personalizado para ajustá-la.".into());
        } else if self.level == "manager" {
            return Err("Somente o gerente pode usar o nível Gerente.".into());
        }
        Ok(())
    }

    pub fn cli_arguments(&self) -> Vec<String> {
        vec![
            "-c".into(),
            format!("model=\"{}\"", self.model),
            "-c".into(),
            format!("model_reasoning_effort=\"{}\"", self.reasoning_effort),
        ]
    }

    pub fn apply_thread(&self, params: &mut Value) {
        params["model"] = json!(self.model);
        params["config"] = json!({"model_reasoning_effort":self.reasoning_effort});
    }

    pub fn turn_params(&self, thread_id: &str, message: &str) -> Value {
        json!({"threadId":thread_id,"model":self.model,"effort":self.reasoning_effort,
            "input":[{"type":"text","text":message,"text_elements":[]}]})
    }

    pub fn verify_catalog(&self, models: &[Value]) -> Result<(), String> {
        let model = models.iter().find(|m| m["model"].as_str() == Some(&self.model))
            .ok_or_else(|| format!("O modelo {} não está disponível no catálogo deste Codex. Ajuste a pessoa ou o login; nenhum modelo substituto foi escolhido.", self.model))?;
        if !model["supportedReasoningEfforts"]
            .as_array()
            .is_some_and(|efforts| {
                efforts
                    .iter()
                    .any(|e| e["reasoningEffort"].as_str() == Some(&self.reasoning_effort))
            })
        {
            return Err(format!(
                "{} não oferece raciocínio {} neste Codex.",
                self.model, self.reasoning_effort
            ));
        }
        Ok(())
    }

    pub fn verify_response(&self, response: &Value) -> Result<(), String> {
        if response["model"].as_str() != Some(&self.model)
            || response["reasoningEffort"].as_str() != Some(&self.reasoning_effort)
        {
            return Err(format!("O Codex não confirmou a configuração solicitada: {} / {}. A execução não foi iniciada.", self.model, self.reasoning_effort));
        }
        Ok(())
    }
}

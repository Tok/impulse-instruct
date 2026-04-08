// ─── tests/helpers_tests.rs ──────────────────────────────────────────────────
// Tests for helper functions: spawn_agent, connect_control.

#[cfg(test)]
mod spawn_agent_tests {
    use crate::state::{AgentRole, AppState, ModuleKind, PortKind, spawn_agent};

    #[test]
    fn spawn_agent_adds_module_and_state() {
        let s = AppState::default();
        let initial_agents = s.llm_agents.len();
        let (s, id) = spawn_agent(s, "TestBot", &[], AgentRole::Producer, None);
        assert_eq!(s.llm_agents.len(), initial_agents + 1);
        assert!(
            s.rack
                .modules
                .iter()
                .any(|m| m.id == id && m.kind == ModuleKind::LlmAgent)
        );
        assert_eq!(s.llm_agents.last().unwrap().persona_name, "TestBot");
    }

    #[test]
    fn spawn_agent_empty_scope_wires_all_controllable() {
        let s = AppState::default();
        let (s, id) = spawn_agent(s, "FullBot", &[], AgentRole::Producer, None);
        let control_cables: Vec<_> = s
            .rack
            .cables
            .iter()
            .filter(|c| c.from.module_id == id && c.from.kind == PortKind::Control)
            .collect();
        assert!(!control_cables.is_empty(), "should have control cables");
    }

    #[test]
    fn spawn_agent_scoped_wires_only_matching() {
        let s = AppState::default();
        let scope = vec!["bass".to_string()];
        let (s, id) = spawn_agent(s, "BassBot", &scope, AgentRole::Specialist, None);
        let targets: Vec<u32> = s
            .rack
            .cables
            .iter()
            .filter(|c| c.from.module_id == id && c.from.kind == PortKind::Control)
            .map(|c| c.to.module_id)
            .collect();
        for tid in &targets {
            let m = s.rack.modules.iter().find(|m| m.id == *tid).unwrap();
            assert_eq!(
                m.kind,
                ModuleKind::AcidBass,
                "scoped agent should only wire to bass"
            );
        }
    }

    #[test]
    fn spawn_agent_with_model_path() {
        let s = AppState::default();
        let (s, _) = spawn_agent(
            s,
            "ModelBot",
            &[],
            AgentRole::Producer,
            Some("models/test.gguf".to_string()),
        );
        let agent = s.llm_agents.last().unwrap();
        assert_eq!(agent.model_path, Some("models/test.gguf".to_string()));
    }

    #[test]
    fn spawn_agent_sets_role() {
        let s = AppState::default();
        let (s, _) = spawn_agent(s, "MC", &[], AgentRole::Mc, None);
        assert_eq!(s.llm_agents.last().unwrap().role, AgentRole::Mc);
    }
}

#[cfg(test)]
mod connect_control_tests {
    use crate::state::{ModuleKind, PortDir, PortKind, RackState};

    #[test]
    fn connect_control_creates_cable_with_correct_ports() {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
        };
        let agent_id = rack.add_module(ModuleKind::LlmAgent);
        let bass_id = rack.add_module(ModuleKind::AcidBass);

        rack.connect_control(agent_id, bass_id);

        assert_eq!(rack.cables.len(), 1);
        let cable = &rack.cables[0];
        assert_eq!(cable.from.module_id, agent_id);
        assert_eq!(cable.from.dir, PortDir::Out);
        assert_eq!(cable.from.kind, PortKind::Control);
        assert_eq!(cable.to.module_id, bass_id);
        assert_eq!(cable.to.dir, PortDir::In);
        assert_eq!(cable.to.kind, PortKind::Control);
    }

    #[test]
    fn connect_control_multiple_targets() {
        let mut rack = RackState {
            modules: Vec::new(),
            cables: Vec::new(),
            next_id: 1,
        };
        let agent_id = rack.add_module(ModuleKind::LlmAgent);
        let bass_id = rack.add_module(ModuleKind::AcidBass);
        let drum_id = rack.add_module(ModuleKind::DrumKit808);

        rack.connect_control(agent_id, bass_id);
        rack.connect_control(agent_id, drum_id);

        assert_eq!(rack.cables.len(), 2);
        assert_eq!(rack.cables[0].to.module_id, bass_id);
        assert_eq!(rack.cables[1].to.module_id, drum_id);
    }
}

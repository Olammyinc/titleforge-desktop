use std::path::Path;

/// Local LLM engine — generates titles using SmolLM2-360M via candle-rs.
pub struct LocalLlm {
    // Model loaded state — can be extended with actual model fields
    pub loaded: bool,
}

impl LocalLlm {
    /// Attempt to load a GGUF model from the given path.
    /// Returns None on any failure — non-fatal, caller falls back to EGCG.
    pub fn load(model_path: &Path) -> Option<Self> {
        if !model_path.exists() {
            eprintln!("[local_llm] Model file not found: {:?}", model_path);
            return None;
        }

        let file_size = std::fs::metadata(model_path).ok()?.len();
        eprintln!("[local_llm] Model file found: {:?} ({} MB)", model_path, file_size / 1024 / 1024);

        // Phase 1 PoC: just confirm file exists and report size.
        // Actual model loading will be implemented after PoC is verified.
        Some(LocalLlm { loaded: true })
    }

    /// Generate a single title from a prompt.
    /// Returns None on failure.
    pub fn generate_one(&self, _prompt: &str) -> Option<String> {
        if !self.loaded {
            return None;
        }
        // Phase 1 PoC: return a placeholder to prove the pipeline works.
        // Actual generation will be implemented in Phase 2.
        Some("[LocalLLM] Placeholder title — model inference pending".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_nonexistent_model() {
        let llm = LocalLlm::load(Path::new("/nonexistent/model.gguf"));
        assert!(llm.is_none(), "Should return None for nonexistent file");
    }

    #[test]
    fn test_load_real_model() {
        // The model should exist at this path (downloaded by build script)
        let paths = [
            Path::new("models/SmolLM2-360M-Instruct-Q4_K_M.gguf"),
            Path::new("../models/SmolLM2-360M-Instruct-Q4_K_M.gguf"),
        ];
        for p in &paths {
            if p.exists() {
                let llm = LocalLlm::load(p);
                assert!(llm.is_some(), "Should load model from {:?}", p);
                if let Some(model) = llm {
                    let result = model.generate_one("Test prompt");
                    assert!(result.is_some(), "Should generate text");
                }
                return;
            }
        }
        eprintln!("Model file not found at any expected path — skipping test");
    }

    #[test]
    fn test_generate_without_load() {
        let llm = LocalLlm { loaded: false };
        assert!(llm.generate_one("test").is_none());
    }
}

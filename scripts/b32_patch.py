#!/usr/bin/env python3
from pathlib import Path

path = Path(__file__).resolve().parent.parent / "src" / "self_update.rs"
text = path.read_text()
old = """            local_path: None,
            provenance: None,
        }
    }

    #[test]
    fn a_bootstrap_link_dir_is_recorded_and_placed()"""
new = """            local_path: None,
            trust: crate::model::TrustResult::default(),
            retained: Vec::new(),
            provenance: None,
        }
    }

    #[test]
    fn a_bootstrap_link_dir_is_recorded_and_placed()"""
if old not in text:
    raise SystemExit("installed_ketch patch target not found")
path.write_text(text.replace(old, new, 1))
print("fixed installed_ketch")

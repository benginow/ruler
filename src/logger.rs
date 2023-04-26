use std::{
    fs::{self, OpenOptions},
    time::{Duration, Instant},
};

use serde_json::{json, Value};

use crate::{count_lines, enumo::Ruleset, DeriveType, Limits, SynthLanguage};

fn add_json_to_file(json: Value) {
    let path = "nightly/data/output.json";
    std::fs::create_dir_all("nightly/data").unwrap_or_else(|e| panic!("Error creating dir: {}", e));

    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(path)
        .expect("Unable to open or create file");

    let s = fs::read_to_string(path).expect("Unable to read file");

    let mut contents: Vec<Value> = if s.is_empty() {
        vec![]
    } else {
        serde_json::from_str(&s).expect("Unable to parse json")
    };

    contents.push(json);

    std::fs::write(path, serde_json::to_string_pretty(&contents).unwrap())
        .expect("Unable to write to json file");
}

pub fn write_baseline<L: SynthLanguage>(
    ruleset: &Ruleset<L>,
    spec_name: &str,
    baseline: &Ruleset<L>,
    baseline_name: &str,
    time: Duration,
) {
    let skip_derive = vec![
        ("herbie", "rational_replicate"),
        ("herbie", "rational_best"),
        ("halide", "halide"),
        ("halide", "oopsla halide (1 iter)"),
        ("oopsla halide (1 iter)", "halide"),
    ];
    let loc = count_lines(spec_name)
        .map(|x| x.to_string())
        .unwrap_or_else(|| "-".to_string());

    let enumo_derives_baseline = if skip_derive.contains(&(spec_name, baseline_name)) {
        json!({})
    } else {
        json!({
            "lhs": get_derivability(ruleset, baseline, DeriveType::Lhs),
            "lhs_rhs": get_derivability(ruleset, baseline, DeriveType::LhsAndRhs)
        })
    };

    let baseline_derives_enumo = if skip_derive.contains(&(spec_name, baseline_name)) {
        json!({})
    } else {
        json!({
            "lhs": get_derivability(baseline, ruleset, DeriveType::Lhs),
            "lhs_rhs": get_derivability(baseline, ruleset, DeriveType::LhsAndRhs)
        })
    };

    let row = json!({
      "spec_name": spec_name,
      "baseline_name": baseline_name,
      "loc": loc,
      "rules": ruleset.to_str_vec(),
      "time": time.as_secs_f64(),
      "derivability": json!({
        "enumo_derives_baseline": enumo_derives_baseline,
        "baseline_derives_enumo": baseline_derives_enumo
      })
    });

    add_json_to_file(row)
}

pub fn write_bv_derivability<L: SynthLanguage>(
    domain: &str,
    gen_rules: Ruleset<L>,
    gen_time: Duration,
    ported_bv4_rules: Ruleset<L>,
) {
    // Validate bv4 rules for this domain
    let start = Instant::now();
    let (sound_bv4, _) = ported_bv4_rules.partition(|rule| rule.is_valid());
    let validate_time = start.elapsed();

    // Compute derivability
    let start = Instant::now();
    let (can, cannot) = sound_bv4.derive(DeriveType::LhsAndRhs, &gen_rules, Limits::deriving());
    let derive_time = start.elapsed();
    let lhsrhs = json!({
        "can": can.to_str_vec(),
        "cannot": cannot.to_str_vec(),
        "time": derive_time.as_secs_f64()
    });

    let start = Instant::now();
    let (can, cannot) = sound_bv4.derive(DeriveType::Lhs, &gen_rules, Limits::deriving());
    let derive_time = start.elapsed();
    let lhs = json!({
        "can": can.to_str_vec(),
        "cannot": cannot.to_str_vec(),
        "time": derive_time.as_secs_f64()
    });

    add_json_to_file(json!({
        "domain": domain,
        "direct_gen": json!({
            "rules": gen_rules.to_str_vec(),
            "time": gen_time.as_secs_f64()
        }),
        "from_bv4": json!({
            "rules": sound_bv4.to_str_vec(),
            "time": validate_time.as_secs_f64()
        }),
        "derivability": json!({
            "lhs": lhs,
            "lhs_rhs": lhsrhs
        })
    }))
}

fn get_derivability<L: SynthLanguage>(
    ruleset: &Ruleset<L>,
    against: &Ruleset<L>,
    derive_type: DeriveType,
) -> Value {
    let start = Instant::now();
    let (can, cannot) = ruleset.derive(derive_type, against, Limits::deriving());
    let elapsed = start.elapsed();

    json!({
        "derive_type": derive_type,
        "can": can.to_str_vec(),
        "cannot": cannot.to_str_vec(),
        "time": elapsed.as_secs_f64()
    })
}

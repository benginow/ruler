use std::{io::Write, sync::Arc, time::Duration};

use egg::{AstSize, EClass, Extractor, RecExpr, Rewrite, Runner, StopReason};

use crate::{
    CVec, EGraph, Equality, HashMap, Id, IndexMap, Signature, SynthAnalysis, SynthLanguage,
    ValidationResult,
};

use super::Workload;

#[derive(Clone, Debug)]
pub struct Ruleset<L: SynthLanguage>(pub IndexMap<Arc<str>, Equality<L>>);

impl<L: SynthLanguage> PartialEq for Ruleset<L> {
    fn eq(&self, other: &Self) -> bool {
        if self.0.len() != other.0.len() {
            return false;
        }
        for ((name1, _), (name2, _)) in self.0.iter().zip(other.0.iter()) {
            if name1 != name2 {
                return false;
            }
        }
        true
    }
}

impl<L: SynthLanguage> Default for Ruleset<L> {
    fn default() -> Self {
        Self(IndexMap::default())
    }
}

impl<L: SynthLanguage> Ruleset<L> {
    pub fn from_str_vec(ss: &[&str]) -> Self {
        let mut map = IndexMap::default();
        let eqs: Vec<Equality<L>> = ss.iter().map(|s| s.parse().unwrap()).collect();
        for eq in eqs {
            map.insert(eq.name.clone(), eq);
        }
        Ruleset(map)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn add(&mut self, eq: Equality<L>) {
        self.0.insert(eq.name.clone(), eq);
    }

    pub fn remove_all(&mut self, other: Self) {
        for (name, _) in other.0 {
            self.0.remove(&name);
        }
    }

    pub fn extend(&mut self, other: Self) {
        self.0.extend(other.0)
    }

    pub fn insert(&mut self, eq: Equality<L>) {
        self.0.insert(eq.name.clone(), eq);
    }

    pub fn to_file(&self, filename: &str) {
        let mut file = std::fs::File::create(filename)
            .unwrap_or_else(|_| panic!("Failed to open '{}'", filename));
        for (name, _) in &self.0 {
            writeln!(file, "{}", name).expect("Unable to write");
        }
    }

    pub fn from_file(filename: &str) -> Self {
        let infile = std::fs::File::open(filename).expect("can't open file");
        let reader = std::io::BufReader::new(infile);
        let mut eqs = IndexMap::default();
        for line in std::io::BufRead::lines(reader) {
            let line = line.unwrap();
            let eq = line.parse::<Equality<L>>().unwrap();
            eqs.insert(eq.name.clone(), eq);
        }
        Self(eqs)
    }

    pub fn partition_sat(&self) -> (Self, Self) {
        let mut sat = IndexMap::default();
        let mut other = IndexMap::default();

        for (name, eq) in &self.0 {
            if eq.is_saturating() {
                sat.insert(name.clone(), eq.clone());
            } else {
                other.insert(name.clone(), eq.clone());
            }
        }

        (Ruleset(sat), Ruleset(other))
    }

    fn mk_runner_with_limits(
        egraph: EGraph<L, SynthAnalysis>,
        node_limit: usize,
        iter_limit: usize,
        time_limit: u64,
    ) -> Runner<L, SynthAnalysis> {
        Runner::default()
            .with_scheduler(egg::SimpleScheduler)
            .with_node_limit(node_limit)
            .with_iter_limit(iter_limit)
            .with_time_limit(Duration::from_secs(time_limit))
            .with_egraph(egraph)
    }

    fn mk_runner(egraph: EGraph<L, SynthAnalysis>) -> Runner<L, SynthAnalysis> {
        Ruleset::mk_runner_with_limits(egraph, 300000, 2, 30)
    }

    fn runner_with_roots(
        runner: Runner<L, SynthAnalysis>,
        lhs: &RecExpr<L>,
        rhs: &RecExpr<L>,
    ) -> Runner<L, SynthAnalysis> {
        runner
            .with_expr(lhs)
            .with_expr(rhs)
            .with_scheduler(egg::SimpleScheduler)
            .with_hook(|r| {
                if r.egraph.find(r.roots[0]) == r.egraph.find(r.roots[1]) {
                    Err("Done".to_owned())
                } else {
                    Ok(())
                }
            })
    }

    pub fn compress_egraph_with_limits(
        &self,
        egraph: EGraph<L, SynthAnalysis>,
        node_limit: usize,
        iter_limit: usize,
        time_limit: u64,
    ) -> (EGraph<L, SynthAnalysis>, HashMap<Id, Vec<Id>>, StopReason) {
        let mut runner = Ruleset::mk_runner_with_limits(egraph, node_limit, iter_limit, time_limit);
        let ids: Vec<Id> = runner.egraph.classes().map(|c| c.id).collect();
        let rewrites: Vec<&Rewrite<L, SynthAnalysis>> =
            self.0.values().map(|eq| &eq.rewrite).collect();
        runner = runner.run(rewrites);
        let stop_reason = runner.stop_reason.unwrap();

        let mut found_unions = HashMap::default();
        for id in ids {
            let new_id = runner.egraph.find(id);
            found_unions.entry(new_id).or_insert_with(Vec::new).push(id);
        }

        runner.egraph.rebuild();
        (runner.egraph, found_unions, stop_reason)
    }

    pub fn compress_egraph(
        &self,
        egraph: EGraph<L, SynthAnalysis>,
    ) -> (EGraph<L, SynthAnalysis>, HashMap<Id, Vec<Id>>, StopReason) {
        self.compress_egraph_with_limits(egraph, 1000000, 3, 30)
    }

    pub fn compress_workload(&self, workload: Workload) -> EGraph<L, SynthAnalysis> {
        let mut egraph = workload.to_egraph();
        let (_, unions, _) = self.compress_egraph(egraph.clone());
        for ids in unions.values() {
            if ids.len() > 1 {
                let first = ids[0];
                for id in &ids[1..] {
                    egraph.union(first, *id);
                }
            }
        }
        egraph.rebuild();

        egraph
    }

    pub fn cvec_match(egraph: &EGraph<L, SynthAnalysis>) -> Self {
        // cvecs [𝑎1, . . . , 𝑎𝑛] and [𝑏1, . . . , 𝑏𝑛] match iff:
        // ∀𝑖. 𝑎𝑖 = 𝑏𝑖 ∨ 𝑎𝑖 = null ∨ 𝑏𝑖 = null and ∃𝑖. 𝑎𝑖 = 𝑏𝑖 ∧ 𝑎𝑖 ≠ null ∧ 𝑏𝑖 ≠ null

        println!(
            "starting cvec match with {} eclasses",
            egraph.number_of_classes()
        );

        let not_all_none: Vec<&EClass<L, Signature<L>>> = egraph
            .classes()
            .filter(|x| x.data.cvec.iter().any(|v| v.is_some()))
            .collect();

        let compare = |cvec1: &CVec<L>, cvec2: &CVec<L>| -> bool {
            for tup in cvec1.iter().zip(cvec2) {
                match tup {
                    (Some(a), Some(b)) if a != b => return false,
                    _ => (),
                }
            }
            true
        };
        let mut candidates = Ruleset::default();
        let extract = Extractor::new(egraph, AstSize);
        for class1 in &not_all_none {
            for class2 in &not_all_none {
                if class1.id == class2.id {
                    continue;
                }
                if compare(&class1.data.cvec, &class2.data.cvec) {
                    let (_, e1) = extract.find_best(class1.id);
                    let (_, e2) = extract.find_best(class2.id);
                    if let Some(eq) = Equality::new(&e1, &e2) {
                        candidates.insert(eq);
                    }
                    if let Some(eq) = Equality::new(&e2, &e1) {
                        candidates.insert(eq);
                    }
                }
            }
        }
        candidates
    }

    fn select(&mut self, step_size: usize) -> Self {
        let mut chosen = Self::default();
        self.0
            .sort_by(|_, eq1, _, eq2| eq1.score().cmp(&eq2.score()));

        // 2. insert step_size best candidates into self.new_rws
        let mut selected: Ruleset<L> = Default::default();
        while selected.len() < step_size {
            let popped = self.0.pop();
            if let Some((_, eq)) = popped {
                if let ValidationResult::Valid = L::validate(&eq.lhs, &eq.rhs) {
                    selected.insert(eq);
                }
            } else {
                break;
            }
        }
        chosen.extend(selected);

        // 3. return chosen candidates
        chosen
    }

    fn shrink(&mut self, chosen: &Self) {
        // 1. make new egraph
        // let mut egraph: EGraph<L, SynthAnalysis> = EGraph::default();
        let mut egraph = EGraph::default();

        let mut initial = vec![];
        // 2. insert lhs and rhs of all candidates as roots
        for eq in self.0.values() {
            let lhs = egraph.add_expr(&L::instantiate(&eq.lhs));
            let rhs = egraph.add_expr(&L::instantiate(&eq.rhs));
            initial.push((lhs, rhs));
        }

        // 3. compress with the rules we've chosen so far
        (egraph, _, _) = chosen.compress_egraph(egraph);

        // 4. go through candidates and if they have merged, then
        // they are no longer candidates
        let extract = Extractor::new(&egraph, AstSize);
        self.0 = Default::default();
        for (l_id, r_id) in initial {
            if egraph.find(l_id) == egraph.find(r_id) {
                // candidate has merged (derivable from other rewrites)
                continue;
            }
            let (_, left) = extract.find_best(l_id);
            let (_, right) = extract.find_best(r_id);
            if let Some(eq) = Equality::new(&left, &right) {
                self.insert(eq);
            }
        }
    }

    pub fn minimize(&mut self, prior: Ruleset<L>) -> Self {
        let mut chosen = prior.clone();
        let step_size = 1;
        while !self.is_empty() {
            let selected = self.select(step_size);
            chosen.extend(selected.clone());
            self.shrink(&chosen);
        }
        // Return only the new rules
        chosen.remove_all(prior);

        chosen
    }

    pub fn derive(&self, against: Self, iter_limit: usize) -> (Self, Self) {
        let (sat, other) = self.partition_sat();
        let sat: Vec<Rewrite<L, SynthAnalysis>> =
            sat.0.iter().map(|(_, eq)| eq.rewrite.clone()).collect();
        let other: Vec<Rewrite<L, SynthAnalysis>> =
            other.0.iter().map(|(_, eq)| eq.rewrite.clone()).collect();

        let mut derivable = IndexMap::default();
        let mut not_derivable = IndexMap::default();

        against.0.into_iter().for_each(|(_, eq)| {
            let l = L::instantiate(&eq.lhs);
            let r = L::instantiate(&eq.rhs);

            let mut runner = Self::runner_with_roots(Self::mk_runner(Default::default()), &l, &r);
            let mut l_id;
            let mut r_id;
            for _ in 0..iter_limit {
                // Sat
                runner = Self::runner_with_roots(Self::mk_runner(runner.egraph), &l, &r)
                    .with_node_limit(usize::MAX)
                    .with_time_limit(Duration::from_secs(30))
                    .with_iter_limit(100)
                    .run(&sat);

                l_id = runner.egraph.find(runner.roots[0]);
                r_id = runner.egraph.find(runner.roots[1]);

                if l_id == r_id {
                    break;
                }

                // Other
                runner = Self::runner_with_roots(Self::mk_runner(runner.egraph), &l, &r)
                    .with_iter_limit(1)
                    .run(&other);

                l_id = runner.egraph.find(runner.roots[0]);
                r_id = runner.egraph.find(runner.roots[1]);

                if l_id == r_id {
                    break;
                }
            }
            // One more sat
            runner = Self::runner_with_roots(Self::mk_runner(runner.egraph), &l, &r)
                .with_node_limit(usize::MAX)
                .with_time_limit(Duration::from_secs(30))
                .with_iter_limit(100)
                .run(&sat);
            l_id = runner.egraph.find(runner.roots[0]);
            r_id = runner.egraph.find(runner.roots[1]);
            if l_id == r_id {
                derivable.insert(eq.name.clone(), eq);
            } else {
                not_derivable.insert(eq.name.clone(), eq);
            }
        });

        println!(
            "{} rules are derivable, {} are not",
            derivable.len(),
            not_derivable.len()
        );

        (Self(derivable), Self(not_derivable))
    }
}

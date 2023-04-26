function load() {
  document.getElementById("baseline_table").innerHTML = ConvertJsonToTable(
    getBaseline("oopsla")
  );
}

function onLoadBv() {
  document.getElementById("container").innerHTML = ConvertJsonToTable(
    getBvData()
  );
}

function getBvData() {
  let exps = data.filter((row) => !!row.from_bv4);
  let keys = {
    Domain: (row) => row.domain,
    Generated: (row) => row.direct_gen.rules.length,
    "Gen Time (s)": (row) => tryRound(row.direct_gen.time),
    "From BV4": (row) => row.from_bv4.rules.length,
    "From BV4 Time (s)": (row) => tryRound(row.from_bv4.time),
    LHS: (row) => getDerivability(row.derivability.lhs),
    "LHS Time": (row) => tryRound(row.derivability.lhs.time, 3),
    "LHS Missing": (row) => formatRules(row.derivability.lhs.cannot),
    "LHS-RHS": (row) => getDerivability(row.derivability.lhs_rhs),
    "LHS-RHS Time": (row) => tryRound(row.derivability.lhs_rhs.time, 3),
    "LHS-RHS Missing": (row) => formatRules(row.derivability.lhs_rhs.cannot),
  };
  let tableData = [];
  exps.forEach((row) => {
    let newRow = {};
    Object.entries(keys).forEach(([key, f]) => {
      newRow[key] = tryRound(f(row));
    });
    tableData.push(newRow);
  });
  return tableData;
}

function getBaseline(name) {
  let keys = {
    Baseline: (row) => row.baseline_name,
    "Enumo Spec": (row) => row.spec_name,
    "Enumo LOC": (row) => row.loc,
    "# Enumo": (row) => row.rules.length,
    "Time (s)": (row) => row.time,
    "Enumo Derives Baseline (LHS / LHSRHS)": (row) =>
      `${getDerivability(
        row.derivability.enumo_derives_baseline.lhs
      )} / ${getDerivability(row.derivability.enumo_derives_baseline.lhs_rhs)}`,
    "Enumo derives Baseline Time (s)": (row) =>
      `${tryRound(
        row.derivability.enumo_derives_baseline.lhs.time,
        3
      )} / ${tryRound(
        row.derivability.enumo_derives_baseline.lhs_rhs.time,
        3
      )}`,
    "Baseline Derives Enumo (LHS / LHSRHS)": (row) =>
      `${getDerivability(
        row.derivability.baseline_derives_enumo.lhs
      )} / ${getDerivability(row.derivability.baseline_derives_enumo.lhs_rhs)}`,
    "Baseline derives Enumo Time (s)": (row) =>
      `${tryRound(
        row.derivability.baseline_derives_enumo.lhs.time,
        3
      )} / ${tryRound(
        row.derivability.baseline_derives_enumo.lhs_rhs.time,
        3
      )}`,
  };
  let tableData = [];
  data.forEach((row) => {
    if (!row["baseline_name"]?.includes(name)) {
      return;
    }
    let newRow = {};
    Object.entries(keys).forEach(([key, f]) => {
      newRow[key] = tryRound(f(row));
    });
    tableData.push(newRow);
  });
  return tableData;
}

function getDerivability(o) {
  let total = o.can.length + o.cannot.length;
  return toPercentage(o.can.length, total, 1);
}

function tryRound(v, precision) {
  if (typeof v == "number") {
    if (v % 1 == 0) {
      return v;
    } else {
      return v.toFixed(precision || 2);
    }
  } else {
    return v;
  }
}

function toPercentage(n, d, decimals) {
  return (
    (tryRound(n / d, decimals + 2 || 2) * 100)
      .toFixed(decimals || 0)
      .toString() + "%"
  );
}

function formatRules(rules) {
  let bidir = [];
  rules.forEach((rule, i) => {
    let [left, right] = rule.split(" ==> ");
    if (rules.includes(`${right} ==> ${left}`)) {
      bidir.push(`${left} <=> ${right}`);
      rules.splice(i, 1);
    } else {
      bidir.push(`${left} ==> ${right}`);
    }
  });
  return bidir.join("<br />");
}

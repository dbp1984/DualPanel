use std::{collections::HashSet, fs, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use anyhow::{anyhow, Result};
use regex::{Regex, RegexBuilder};

#[derive(Clone, Debug)]
pub struct RenameRule {
    pub template: String,
    pub find: String,
    pub replace: String,
    pub prefix: String,
    pub suffix: String,
    pub extension: String,
    pub counter_start: usize,
    pub counter_step: usize,
    pub counter_width: usize,
    pub regex: bool,
    pub match_case: bool,
}
impl Default for RenameRule {
    fn default() -> Self { Self { template:"[N]".into(), find:String::new(), replace:String::new(), prefix:String::new(), suffix:String::new(), extension:String::new(), counter_start:1, counter_step:1, counter_width:1, regex:false, match_case:true } }
}

#[derive(Clone, Debug)]
pub struct RenamePlan { pub from: PathBuf, pub to: PathBuf }

fn split_name(path: &Path) -> (String, String, String) {
    let full = path.file_name().and_then(|x| x.to_str()).unwrap_or("").to_string();
    let stem = path.file_stem().and_then(|x| x.to_str()).unwrap_or("").to_string();
    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_string();
    (full, stem, ext)
}

fn replace_text(s: &str, find: &str, replacement: &str, regex: bool, match_case: bool) -> Result<String> {
    if find.is_empty() { return Ok(s.to_string()); }
    if regex {
        let re = RegexBuilder::new(find).case_insensitive(!match_case).build()?;
        Ok(re.replace_all(s, replacement).to_string())
    } else if match_case { Ok(s.replace(find, replacement)) }
    else {
        let re = RegexBuilder::new(&regex::escape(find)).case_insensitive(true).build()?;
        Ok(re.replace_all(s, replacement).to_string())
    }
}

pub fn plan(paths: &[PathBuf], rule: &RenameRule) -> Result<Vec<RenamePlan>> {
    let mut out = Vec::with_capacity(paths.len());
    let mut targets = HashSet::new();
    for (i, from) in paths.iter().enumerate() {
        let (full, stem, ext) = split_name(from);
        let processed = replace_text(&stem, &rule.find, &rule.replace, rule.regex, rule.match_case)?;
        let n = rule.counter_start + i * rule.counter_step;
        let default_counter = format!("{:0width$}", n, width=rule.counter_width.max(1));
        let mut name = rule.template.replace("[N]", &processed).replace("[O]", &stem).replace("[F]", &full).replace("[E]", &ext).replace("[C]", &default_counter);
        // [C3], [C4]...
        let cre = Regex::new(r"\[C(\d+)\]")?;
        name = cre.replace_all(&name, |caps: &regex::Captures| { let w = caps[1].parse::<usize>().unwrap_or(rule.counter_width.max(1)); format!("{:0width$}", n, width=w) }).to_string();
        name = format!("{}{}{}", rule.prefix, name, rule.suffix);
        let wanted_ext = if rule.extension.trim().is_empty() { ext.clone() } else { rule.extension.trim().trim_start_matches('.').to_string() };
        // Avoid duplicating extension when [F] was used or template already ends with it.
        if !wanted_ext.is_empty() && !name.to_lowercase().ends_with(&format!(".{}", wanted_ext.to_lowercase())) { name.push('.'); name.push_str(&wanted_ext); }
        let parent = from.parent().unwrap_or_else(|| Path::new("."));
        let to = parent.join(name);
        if !targets.insert(to.clone()) { return Err(anyhow!("Two files would get the same name: {}", to.display())); }
        if to != *from && to.exists() && !paths.iter().any(|p| p == &to) { return Err(anyhow!("Destination already exists: {}", to.display())); }
        out.push(RenamePlan { from: from.clone(), to });
    }
    Ok(out)
}

fn unique_temp_path(parent: &Path, idx: usize) -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    parent.join(format!(".dbp-rename-{stamp}-{idx}.tmp"))
}

pub fn apply(plans: &[RenamePlan]) -> Result<()> {
    let mut staged: Vec<(PathBuf, PathBuf, PathBuf)> = Vec::new();
    for (i, p) in plans.iter().enumerate() {
        if p.from == p.to { continue; }
        let tmp = unique_temp_path(p.from.parent().unwrap_or_else(|| Path::new(".")), i);
        fs::rename(&p.from, &tmp)?;
        staged.push((p.from.clone(), tmp, p.to.clone()));
    }
    let mut completed = 0usize;
    for (_, tmp, target) in &staged {
        if let Err(e) = fs::rename(tmp, target) {
            let mut rolled = Vec::new();
            for (j,(orig,_,final_path)) in staged.iter().take(completed).enumerate() {
                let rb = unique_temp_path(final_path.parent().unwrap_or_else(|| Path::new(".")), 100000+j);
                if fs::rename(final_path, &rb).is_ok() { rolled.push((orig.clone(), rb)); }
            }
            for (orig,tmp,_) in staged.iter().skip(completed) { if tmp.exists() { let _ = fs::rename(tmp, orig); } }
            for (orig,rb) in rolled.into_iter().rev() { let _ = fs::rename(rb, orig); }
            return Err(anyhow!("Rename failed at {}: {e}; rollback attempted", target.display()));
        }
        completed += 1;
    }
    Ok(())
}

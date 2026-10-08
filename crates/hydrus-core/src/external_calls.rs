//! Named external calls and argument insertion. Calls use an argument vector,
//! never a shell command; template replacement is once per token and argument.
use crate::url::strings::StringProcessor;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The two jobs offered by the reference callable editor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Pipeline {
    File,
    Url,
}
impl Pipeline {
    /// The list's job column.
    pub fn label(self) -> &'static str {
        match self {
            Self::File => "send single file",
            Self::Url => "send single URL",
        }
    }
    /// The reference's job summary, with its available and expected parameters.
    pub fn description(self) -> String {
        let summary = match self {
            Self::File => "This tells the client how to open a file in another program.",
            Self::Url => "This tells the client how to open a URL in another program.",
        };
        let inputs = self
            .parameters()
            .iter()
            .map(|p| p.label())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "Summary: {summary}\n\nAvailable input parameters: {inputs}\n\nExpected output parameters: none"
        )
    }
    /// What the default OS launch call does, as the reference words it.
    pub fn default_launch_description(self) -> String {
        match self {
            Self::File => {
                let how = if cfg!(windows) {
                    "For Windows, this is a hardcoded system call."
                } else if cfg!(target_os = "macos") {
                    "For macOS, this is \"open %path%\"."
                } else {
                    "For Linux, this is \"xdg-open %path%\"."
                };
                format!(
                    "This will try to launch the file using your OS's default file handler.\n\n{how}"
                )
            }
            Self::Url => "This will try to launch the URL using a library that attempts to figure out your OS's default URL handler. It may lose the \"#anchor\" fragment on the end of an URL.".into(),
        }
    }
    /// Parameters the selected job can supply.
    pub fn parameters(self) -> &'static [Parameter] {
        match self {
            Self::File => &[
                Parameter::Path,
                Parameter::Uri,
                Parameter::Hash,
                Parameter::FileId,
            ],
            Self::Url => &[Parameter::Url],
        }
    }
}
/// A typed input supplied to an external call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Parameter {
    Path,
    Uri,
    Url,
    Paths,
    Uris,
    Hash,
    FileId,
}
impl Parameter {
    /// Reference parameter code at the serialization boundary.
    pub fn code(self) -> i64 {
        match self {
            Self::Path => 0,
            Self::Uri => 1,
            Self::Url => 2,
            Self::Paths => 3,
            Self::Uris => 4,
            Self::Hash => 5,
            Self::FileId => 6,
        }
    }
    /// Decode a reference parameter without allowing unknown runtime meanings.
    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(Self::Path),
            1 => Some(Self::Uri),
            2 => Some(Self::Url),
            3 => Some(Self::Paths),
            4 => Some(Self::Uris),
            5 => Some(Self::Hash),
            6 => Some(Self::FileId),
            _ => None,
        }
    }
    /// Human description in validation and test controls.
    pub fn label(self) -> &'static str {
        match self {
            Self::Path => "file path",
            Self::Uri => "file URI",
            Self::Url => "URL",
            Self::Paths => "file paths",
            Self::Uris => "file URIs",
            Self::Hash => "file hash (sha256)",
            Self::FileId => "file id",
        }
    }
    /// The initial insertion token.
    pub fn token(self) -> &'static str {
        match self {
            Self::Path => "%path%",
            Self::Uri => "%path_uri%",
            Self::Url => "%url%",
            Self::Paths => "%paths%",
            Self::Uris => "%paths_uri%",
            Self::Hash => "%hash%",
            Self::FileId => "%file_id%",
        }
    }
}
/// Processing applied before an input replaces its token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub parameter: Parameter,
    pub token: String,
    pub processor: StringProcessor,
}
impl Rule {
    /// An enabled input using its default token and identity processing.
    pub fn new(parameter: Parameter) -> Self {
        Self {
            parameter,
            token: parameter.token().into(),
            processor: StringProcessor::default(),
        }
    }
}
/// A direct local process call, with a bounded wait or detached lifetime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Process {
    pub executable: String,
    pub arguments: Vec<String>,
    pub rules: Vec<Rule>,
    pub timeout_seconds: u64,
    pub long_lived: bool,
    pub hide_terminal: bool,
    pub text: bool,
}
impl Default for Process {
    fn default() -> Self {
        Self {
            executable: String::new(),
            arguments: Vec::new(),
            rules: Vec::new(),
            timeout_seconds: 15,
            long_lived: false,
            hide_terminal: true,
            text: true,
        }
    }
}
/// The actual operation associated with a named callable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActualCall {
    Process(Process),
    DefaultFile,
    DefaultUrl,
}
/// A named registered external call. Keys identify calls independently of names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Callable {
    pub key: [u8; 32],
    pub name: String,
    pub pipeline: Pipeline,
    pub call: ActualCall,
}
impl Callable {
    /// A detached blank draft for the Add child.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            key: rand::random(),
            name: name.into(),
            pipeline: Pipeline::File,
            call: ActualCall::Process(Process::default()),
        }
    }
    /// Replace an added/imported/duplicated call's identity.
    pub fn regenerate_key(&mut self) {
        self.key = rand::random();
    }
}
/// Registered calls persisted together when Options is accepted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Manager {
    pub calls: Vec<Callable>,
}
/// Test or launch inputs. Lists retain their order through string processing.
pub type Inputs = BTreeMap<Parameter, Vec<String>>;
/// Normalize individual parameter templates as the reference command editor does.
pub fn clean_arguments(arguments: &[String]) -> Vec<String> {
    arguments
        .iter()
        .flat_map(|s| {
            let normalised = s.replace("\r\n", "\n").replace(
                [
                    '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
                    '\u{2029}',
                ],
                "\n",
            );
            normalised
                .split_terminator('\n')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .map(|s| {
            s.split(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}
fn python_strings(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|s| crate::url::string_descriptions::python_repr_str(s))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
impl Process {
    /// Build the final argument vector, with no shell interpretation.
    pub fn command(&self, inputs: &Inputs) -> Result<Vec<String>, String> {
        let mut command = vec![self.executable.clone()];
        let mut used = vec![false; self.rules.len()];
        for template in &self.arguments {
            let mut replacements = Vec::new();
            for (i, rule) in self.rules.iter().enumerate() {
                let Some(index) = template.find(&rule.token) else {
                    continue;
                };
                let values = inputs.get(&rule.parameter).ok_or_else(|| {
                    format!(
                        "The expected input parameter \"{}\" was not in the call arguments!",
                        rule.parameter.label()
                    )
                })?;
                // The reference processor skips failed steps and uses its first result.
                let value = rule
                    .processor
                    .process(values.clone())
                    .unwrap_or_default()
                    .into_iter()
                    .next()
                    .ok_or_else(|| {
                        format!(
                            "The input parameter \"{}\" did not string-process to anything!",
                            python_strings(values)
                        )
                    })?;
                used[i] = true;
                replacements.push((index, rule.token.clone(), value));
            }
            replacements.sort_by(|a, b| b.cmp(a));
            let mut argument = template.clone();
            for (_, token, value) in replacements {
                argument = argument.replacen(&token, &value, 1);
            }
            command.push(argument);
        }
        let mut missing: Vec<_> = self
            .rules
            .iter()
            .zip(used)
            .filter(|(_, used)| !*used)
            .map(|(rule, _)| format!("\"{}\"", rule.parameter.label()))
            .collect();
        if !missing.is_empty() {
            missing.sort();
            let quoted = format!("\"{}\"", missing.join("\", \""));
            let summary = if quoted.chars().count() <= 48 {
                quoted
            } else {
                let leading = format!(
                    "\"{}\" & {} other parameters",
                    missing[0],
                    missing.len() - 1
                );
                if missing.len() > 1 && leading.chars().count() <= 48 {
                    leading
                } else {
                    format!("{} parameters", missing.len())
                }
            };
            return Err(format!(
                "Was set to ask for certain input parameters, but then could not find the associated replacement tokens in the parameter list. Missing parameters were: {summary}"
            ));
        }
        Ok(command)
    }
    /// Warning shown before accepting an incomplete callable draft.
    pub fn validate(&self) -> Result<(), String> {
        if self.executable.is_empty() {
            return Err("No executable path is set!".into());
        }
        if self.rules.is_empty() {
            return Err("No input parameters are being used! This is only appropriate if you just want to send a notification signal every time this event happens.".into());
        }
        for rule in &self.rules {
            if !self.arguments.iter().any(|a| a.contains(&rule.token)) {
                return Err(format!(
                    "The replacement string \"{}\" for input parameter \"{}\" is not in the command template!",
                    rule.token,
                    rule.parameter.label()
                ));
            }
        }
        Ok(())
    }
    /// Import warning for unusually large commands; callers may explicitly accept it.
    pub fn import_warning(&self) -> Option<String> {
        let mut issues = Vec::new();
        if self.executable.chars().count() > 256 {
            issues.push(format!(
                "The executable path is over 256 characters: {}",
                self.executable
            ));
        }
        if self.arguments.len() > 16 {
            issues.push(format!(
                "There are more than 16 parameters: {}",
                python_strings(&self.arguments)
            ));
        }
        if self.arguments.join(" ").chars().count() > 1024 {
            issues.push(format!(
                "The parameters' total length is over 1024 characters: {}",
                python_strings(&self.arguments)
            ));
        }
        (!issues.is_empty()).then(|| issues.join("\n\n"))
    }
}
impl ActualCall {
    /// Read-only list command description.
    pub fn description(&self) -> String {
        match self {
            Self::Process(p) if p.executable.is_empty() => "no call set!".into(),
            Self::Process(p) => format!("CALL: {} {}", p.executable, p.arguments.join(" ")),
            Self::DefaultFile => "-hardcoded- Call OS default file launcher".into(),
            Self::DefaultUrl => "-hardcoded- Call OS default URL launcher".into(),
        }
    }
    /// Required inputs, in the reference rule order.
    pub fn parameters(&self) -> Vec<Parameter> {
        match self {
            Self::Process(p) => p.rules.iter().map(|r| r.parameter).collect(),
            Self::DefaultFile => vec![Parameter::Path],
            Self::DefaultUrl => vec![Parameter::Url],
        }
    }
    /// The preview next to the actual test call button.
    pub fn preview(&self, inputs: &Inputs) -> String {
        match self {
            Self::Process(p) => p
                .command(inputs)
                .map_or_else(|e| format!("Error! {e}"), |v| v.join(" ")),
            Self::DefaultFile | Self::DefaultUrl => {
                let parameter = if matches!(self, Self::DefaultFile) {
                    Parameter::Path
                } else {
                    Parameter::Url
                };
                inputs.get(&parameter).and_then(|v| v.first()).map_or_else(
                    || format!("Error! {}", parameter.code()),
                    |v| format!("Ask OS to open \"{v}\""),
                )
            }
        }
    }
}

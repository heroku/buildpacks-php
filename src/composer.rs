pub(crate) mod platform;

use deref_derive::Deref;
use monostate::MustBe;
use serde::de::{Error, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use serde_with::{formats::PreferOne, serde_as, skip_serializing_none, OneOrMany};
use std::collections::HashMap;
use url::Url;

use std::fmt;
use std::marker::PhantomData;
use std::ops::Not;
use std::path::{Path, PathBuf};

#[derive(Debug, Deref, Serialize, PartialEq)]
struct PhpAssocArray<T>(HashMap<String, T>);
impl<'de, T: Deserialize<'de>> Deserialize<'de> for PhpAssocArray<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EmptyPhpObjectVisitor<T> {
            marker: PhantomData<T>,
        }

        impl<'de, T> Visitor<'de> for EmptyPhpObjectVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = PhpAssocArray<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an empty sequence or an object")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                match seq.next_element::<T>() {
                    Ok(Some(_)) => Err(A::Error::custom("sequence must be empty")), // if a T is in there
                    Ok(None) => Ok(PhpAssocArray(HashMap::new())),
                    Err(_) => Err(A::Error::custom("sequence must be empty")), // if something else is in there
                }
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = HashMap::<String, T>::new();
                while let Some((key, value)) = map.next_entry::<String, T>()? {
                    values.insert(key, value);
                }

                Ok(PhpAssocArray(values))
            }
        }

        let visitor = EmptyPhpObjectVisitor {
            marker: PhantomData,
        };

        deserializer.deserialize_any(visitor)
    }
}

/*pub(crate) enum ComposerDependencyError {
    VersionMustBeAString,
}
impl Display for ComposerDependencyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match (self) {
            Self::VersionMustBeAString => write!(f, "Version must be a string"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ComposerDependency {
    name: String,
    version: String,
}
impl TryFrom<(String, Value)> for ComposerDependency {
    type Error = ComposerDependencyError;

    fn try_from(value: (String, Value)) -> Result<Self, Self::Error> {
        match value.1 {
            Value::String(version) => Ok(ComposerDependency {
                name: value.0,
                version: version.clone(),
            }),
            _ => Err(ComposerDependencyError::VersionMustBeAString),
        }
    }
}

// used with ComposerDependency
fn opt_map_to_opt_vec<'de, T, D>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
where
    T: Deserialize<'de> + TryFrom<(String, Value)>,
    <T as TryFrom<(String, Value)>>::Error: Display,
    D: Deserializer<'de>,
{
    Option::<Map<String, Value>>::deserialize(deserializer).and_then(|optmap| {
        optmap
            .map(|svmap| {
                svmap
                    .into_iter()
                    .map(T::try_from)
                    .collect::<Result<Vec<T>, <T as TryFrom<(String, Value)>>::Error>>()
                    .map_err(D::Error::custom)
            })
            .transpose()
    })
}

// used with ComposerDependency
fn map_to_vec<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    T: Deserialize<'de> + TryFrom<(String, Value)>,
    <T as TryFrom<(String, Value)>>::Error: Display,
    D: Deserializer<'de>,
{
    <Map<String, Value>>::deserialize(deserializer).and_then(|svmap| {
        svmap
            .into_iter()
            .map(T::try_from)
            .collect::<Result<Vec<T>, <T as TryFrom<(String, Value)>>::Error>>()
            .map_err(D::Error::custom)
    })
}
*/

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ComposerRootPackage {
    name: Option<String>,
    version: Option<String>,
    config: Option<Map<String, Value>>,
    minimum_stability: Option<ComposerLiteralStability>,
    prefer_stable: Option<bool>,
    #[serde(flatten)]
    package: ComposerBasePackage,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ComposerPackage {
    name: String,
    version: String,
    #[serde(flatten)]
    package: ComposerBasePackage,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ComposerBasePackage {
    #[serde(rename = "type")]
    kind: Option<String>,
    abandoned: Option<ComposerPackageAbandoned>,
    archive: Option<ComposerPackageArchive>,
    authors: Option<Vec<ComposerPackageAuthor>>,
    autoload: Option<ComposerPackageAutoload>,
    autoload_dev: Option<ComposerPackageAutoload>,
    #[serde_as(as = "Option<OneOrMany<_, PreferOne>>")]
    bin: Option<Vec<String>>,
    conflict: Option<HashMap<String, String>>,
    description: Option<String>,
    dist: Option<ComposerPackageDist>,
    extra: Option<Value>,
    funding: Option<Vec<ComposerPackageFunding>>,
    homepage: Option<Url>,
    include_path: Option<Vec<String>>,
    keywords: Option<Vec<String>>,
    #[serde_as(as = "Option<OneOrMany<_, PreferOne>>")]
    license: Option<Vec<String>>,
    minimum_stability: Option<ComposerStability>,
    non_feature_branches: Option<Vec<String>>,
    prefer_stable: Option<bool>,
    provide: Option<HashMap<String, String>>,
    readme: Option<PathBuf>,
    replace: Option<HashMap<String, String>>,
    repositories: Option<Vec<ComposerRepository>>,
    require: Option<HashMap<String, String>>,
    require_dev: Option<HashMap<String, String>>,
    scripts: Option<HashMap<String, Vec<String>>>,
    scripts_descriptions: Option<HashMap<String, String>>,
    source: Option<ComposerPackageSource>,
    support: Option<HashMap<ComposerPackageSupportType, String>>,
    suggest: Option<HashMap<String, String>>,
    target_dir: Option<String>,
    time: Option<String>, // TODO: "Package release date, in 'YYYY-MM-DD', 'YYYY-MM-DD HH:MM:SS' or 'YYYY-MM-DDTHH:MM:SSZ' format.", but in practice it uses DateTime::__construct(), which can parse a lot of formats
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ComposerConfig {
    #[serde(rename = "cache-files-ttl")]
    cache_files_ttl: Option<u32>,
    #[serde(rename = "discard-changes")]
    discard_changes: Option<bool>,
    #[serde(rename = "allow-plugins")]
    allow_plugins: Option<ComposerConfigAllowPlugins>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum ComposerConfigAllowPlugins {
    Boolean(bool),
    List(HashMap<String, bool>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub(crate) enum ComposerStability {
    Dev = 20,
    Alpha = 15,
    Beta = 10,
    Rc = 5,
    #[default]
    Stable = 0,
}

#[derive(Serialize, Deserialize, Deref, Debug, Clone)]
#[serde(try_from = "String")]
struct ComposerLiteralStability(ComposerStability);
impl TryFrom<String> for ComposerLiteralStability {
    type Error = String;

    fn try_from(v: String) -> Result<Self, Self::Error> {
        match v.as_ref() {
            "stable" => Ok(ComposerLiteralStability(ComposerStability::Stable)),
            "rc" | "RC" => Ok(ComposerLiteralStability(ComposerStability::Rc)),
            "beta" => Ok(ComposerLiteralStability(ComposerStability::Beta)),
            "alpha" => Ok(ComposerLiteralStability(ComposerStability::Alpha)),
            "dev" => Ok(ComposerLiteralStability(ComposerStability::Dev)),
            _ => Err(format!("Invalid stability flag {v}")),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum ComposerPackageAbandoned {
    Bool(bool),
    Alternative(String),
}
impl Default for ComposerPackageAbandoned {
    fn default() -> Self {
        Self::Bool(false)
    }
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Default)]
pub(crate) struct ComposerPackageAuthor {
    name: String,
    email: Option<String>, // TODO: could be EmailAddress, but Composer only warns
    homepage: Option<Url>,
    role: Option<String>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ComposerPackageAutoload {
    // map values for the next two can be string or list of strings
    #[serde_as(as = "Option<HashMap<_, OneOrMany<_, PreferOne>>>")]
    psr_0: Option<HashMap<String, Vec<String>>>,
    #[serde_as(as = "Option<HashMap<_, OneOrMany<_, PreferOne>>>")]
    psr_4: Option<HashMap<String, Vec<String>>>,
    classmap: Option<Vec<String>>,
    files: Option<Vec<String>>,
    exclude_from_classmap: Option<Vec<String>>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Default)]
pub(crate) struct ComposerPackageArchive {
    name: Option<String>,
    exclude: Option<Vec<String>>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ComposerPackageDist {
    #[serde(rename = "type")]
    kind: String,
    url: Url,
    reference: Option<String>,
    shasum: Option<String>,
    mirrors: Option<Vec<ComposerMirror>>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ComposerPackageSource {
    #[serde(rename = "type")]
    kind: String,
    url: Url,
    reference: Option<String>,
    mirrors: Option<Vec<ComposerMirror>>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ComposerMirror {
    url: Url,
    preferred: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct ComposerPackageFunding {
    #[serde(rename = "type")]
    kind: String, // default "other"?
    url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
#[serde(rename_all = "lowercase")]
#[serde(try_from = "String")]
pub(crate) enum ComposerPackageSupportType {
    Email,
    Issues,
    Forum,
    Wiki,
    Irc,
    Chat,
    Source,
    Docs,
    Rss,
    Security,
}
impl TryFrom<String> for ComposerPackageSupportType {
    type Error = String;

    fn try_from(v: String) -> Result<Self, Self::Error> {
        match v.as_ref() {
            "email" => Ok(ComposerPackageSupportType::Email),
            "issues" => Ok(ComposerPackageSupportType::Issues),
            "forum" => Ok(ComposerPackageSupportType::Forum),
            "wiki" => Ok(ComposerPackageSupportType::Wiki),
            "irc" => Ok(ComposerPackageSupportType::Irc),
            "chat" => Ok(ComposerPackageSupportType::Chat),
            "source" => Ok(ComposerPackageSupportType::Source),
            "docs" => Ok(ComposerPackageSupportType::Docs),
            "rss" => Ok(ComposerPackageSupportType::Rss),
            "security" => Ok(ComposerPackageSupportType::Security),
            _ => Err(format!("Invalid support type {v}")),
        }
    }
}

// this is not actually untagged, but we need to mix two entry types:
// 1) internally tagged via "type" for "real" repository types
// 2) { "name": false } to disable a repo (we use that to disable packagist via { "packagist.org": false }
// Tagged enums can have #[serde(other)] only for a unit type: https://github.com/serde-rs/serde/issues/912
// A workaround with tags declared on separate structs (https://stackoverflow.com/a/61219284/162354) doesn't work as explained in https://stackoverflow.com/a/74544853/162354
// The solution is to rely on monostrate's MustBe!
#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub(crate) enum ComposerRepository {
    #[serde(rename_all = "kebab-case")]
    Composer {
        #[serde(rename = "type")]
        kind: MustBe!("composer"),
        url: Url,
        #[serde(rename = "allow_ssl_downgrade")]
        allow_ssl_downgrade: Option<bool>,
        force_lazy_providers: Option<bool>,
        options: Option<Map<String, Value>>,
        #[serde(flatten)]
        filters: ComposerRepositoryFilters,
    },
    #[serde(rename_all = "kebab-case")]
    Path {
        #[serde(rename = "type")]
        kind: MustBe!("path"),
        url: PathBuf, // TODO: Path
        options: Option<Map<String, Value>>,
        #[serde(flatten)]
        filters: ComposerRepositoryFilters,
    },
    #[serde(rename_all = "kebab-case")]
    Package {
        #[serde(rename = "type")]
        kind: MustBe!("package"),
        #[serde_as(as = "OneOrMany<_, PreferOne>")]
        package: Vec<ComposerPackage>,
        #[serde(flatten)]
        filters: ComposerRepositoryFilters,
    },
    #[serde(rename_all = "kebab-case")]
    Url {
        #[serde(rename = "type")]
        kind: String,
        url: Url,
        #[serde(flatten)]
        filters: ComposerRepositoryFilters,
        #[serde(flatten)]
        extra: HashMap<String, Value>,
    },
    #[serde(rename_all = "kebab-case")]
    Other {
        #[serde(rename = "type")]
        kind: String,
        #[serde(flatten)]
        filters: ComposerRepositoryFilters,
        #[serde(flatten)]
        extra: HashMap<String, Value>,
    },
    #[serde(rename_all = "kebab-case")]
    Disabled(HashMap<String, MustBe!(false)>),
}
// FIXME: this should not be in this module, at least not this specialized (symlink => false)
impl From<&Path> for ComposerRepository {
    fn from(value: &Path) -> Self {
        Self::Path {
            kind: Default::default(),
            url: value.into(),
            options: Some(Map::from_iter([("symlink".into(), Value::Bool(false))])),
            filters: ComposerRepositoryFilters {
                canonical: None,
                only: None,
                exclude: None,
            },
        }
    }
}
impl From<Vec<ComposerPackage>> for ComposerRepository {
    fn from(value: Vec<ComposerPackage>) -> Self {
        Self::Package {
            kind: Default::default(),
            package: value,
            filters: ComposerRepositoryFilters {
                canonical: None,
                only: None,
                exclude: None,
            },
        }
    }
}
impl FromIterator<ComposerPackage> for ComposerRepository {
    fn from_iter<T: IntoIterator<Item = ComposerPackage>>(iter: T) -> Self {
        ComposerRepository::from(iter.into_iter().collect::<Vec<_>>())
    }
}

// FIXME: this should not be in this module
fn ensure_heroku_sys_prefix(name: impl AsRef<str>) -> String {
    let name = name.as_ref();
    format!(
        "heroku-sys/{}",
        name.strip_prefix("heroku-sys/").unwrap_or(name)
    )
}
fn split_and_trim_list<'a>(list: &'a str, sep: &'a str) -> impl Iterator<Item = &'a str> {
    list.split(sep)
        .map(str::trim)
        .filter_map(|p| (!p.is_empty()).then_some(p))
}
// FIXME: this should not be in this module, at least not with the ensure_heroku_sys_prefix specialization
impl TryFrom<Url> for ComposerRepository {
    type Error = ();

    fn try_from(value: Url) -> Result<Self, Self::Error> {
        let mut filters = ComposerRepositoryFilters {
            canonical: None,
            only: None,
            exclude: None,
        };
        // allow control of https://getcomposer.org/doc/articles/repository-priorities.md via query args:
        // ?composer-repository-canonical=false
        // ?composer-repository-only=heroku-sys/ext-foo
        // ?composer-repository-exclude=ext-lol
        // FIXME: for 100% parity with Classic, we could support array notation: ?composer-repository-exclude[]=ext-foo&composer-repository-exclude[]=ext-bar
        // TODO: should an empty string for only/exclude query arg generate Some(vec![]), or None? https://github.com/composer/composer/blob/11879ea737978fabb8127616e703e571ff71b184/src/Composer/Repository/FilterRepository.php#L218-L233
        for (k, v) in value.query_pairs() {
            match &*k {
                "composer-repository-canonical" => {
                    filters.canonical = match &*v.trim().to_ascii_lowercase() {
                        "1" | "true" | "on" | "yes" => Some(true),
                        &_ => Some(false),
                    }
                }
                "composer-repository-only" => {
                    filters.only = Some(
                        split_and_trim_list(&*v, ",")
                            .map(ensure_heroku_sys_prefix)
                            .collect(),
                    )
                    .filter(|v: &Vec<String>| !v.is_empty());
                }
                "composer-repository-exclude" => {
                    filters.exclude = Some(
                        split_and_trim_list(&*v, ",")
                            .map(ensure_heroku_sys_prefix)
                            .collect(),
                    )
                    .filter(|v: &Vec<String>| !v.is_empty());
                }
                _ => (),
            }
        }

        if filters.only.is_some() && filters.exclude.is_some() {
            return Err(());
        }

        Ok(Self::Composer {
            kind: Default::default(),
            url: value,
            allow_ssl_downgrade: None,
            force_lazy_providers: None,
            options: None,
            filters,
        })
    }
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
//#[serde(default)]
pub(crate) struct ComposerRepositoryFilters {
    canonical: Option<bool>,
    // FIXME: these are mutually exclusive
    only: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct ComposerLock {
    content_hash: String, // since 1.0: https://github.com/composer/composer/pull/4140
    packages: Vec<ComposerPackage>,
    packages_dev: Vec<ComposerPackage>, // TODO: can it really be null in practice?
    platform: PhpAssocArray<String>,
    platform_dev: PhpAssocArray<String>,
    platform_overrides: Option<HashMap<String, String>>, // since 1.0: https://github.com/composer/composer/commit/a57c51e8d78156612e49dec1c54d3184f260f144
    // aliases: HashMap<String, ComposerPackage>, // since 1.0: https://github.com/composer/composer/pull/350 - TODO: do we need to handle these?
    minimum_stability: ComposerLiteralStability, // since 1.0: https://github.com/composer/composer/pull/592
    stability_flags: PhpAssocArray<ComposerNumericStability>, // since 1.0: https://github.com/composer/composer/pull/592 - FIXME: empty will be JSON array again
    prefer_stable: bool, // since 1.0: https://github.com/composer/composer/pull/3101
    prefer_lowest: bool, // since 1.0: https://github.com/composer/composer/pull/3450
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    plugin_api_version: Option<String>, // since 1.10.0: https://github.com/composer/composer/commit/0b9c658bef426a56dc3971e614028ff5078bcd95
}
impl ComposerLock {
    pub(crate) fn new(plugin_api_version: Option<String>) -> Self {
        Self {
            content_hash: "".to_string(),
            packages: vec![],
            packages_dev: vec![],
            platform: PhpAssocArray(Default::default()),
            platform_dev: PhpAssocArray(Default::default()),
            platform_overrides: None,
            minimum_stability: ComposerLiteralStability(ComposerStability::Stable),
            stability_flags: PhpAssocArray(Default::default()),
            prefer_stable: false,
            prefer_lowest: false,
            plugin_api_version,
        }
    }
}

#[derive(Serialize, Deserialize, Deref, Debug, Clone)]
#[serde(try_from = "u8", into = "u8")]
struct ComposerNumericStability(ComposerStability);
impl From<ComposerNumericStability> for u8 {
    fn from(value: ComposerNumericStability) -> Self {
        value.0 as u8
    }
}
impl TryFrom<u8> for ComposerNumericStability {
    type Error = String;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(ComposerNumericStability(ComposerStability::Stable)),
            5 => Ok(ComposerNumericStability(ComposerStability::Rc)),
            10 => Ok(ComposerNumericStability(ComposerStability::Beta)),
            15 => Ok(ComposerNumericStability(ComposerStability::Alpha)),
            20 => Ok(ComposerNumericStability(ComposerStability::Dev)),
            _ => Err(format!("Invalid stability flag {v}")),
        }
    }
}

// used for deserializing empty [] to a default, also see PhpAssocArray
fn empty_vec_to_default<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + Default,
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VecOrOriginal<T> {
        Original(T),
        Vec(Vec<Value>),
    }

    match VecOrOriginal::deserialize(deserializer)? {
        VecOrOriginal::Vec(v) => {
            if !v.is_empty() {
                Err(D::Error::custom("got unexpected array"))
            } else {
                Ok(<T as Default>::default())
            }
        }
        VecOrOriginal::Original(v) => Ok(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_test::{assert_de_tokens, assert_de_tokens_error, Token};

    #[derive(Debug, PartialEq, Deserialize, Serialize)]
    #[serde(transparent)]
    struct EVT {
        #[serde(deserialize_with = "empty_vec_to_default")]
        foo: HashMap<String, bool>,
    }

    #[derive(Debug, PartialEq, Deserialize, Serialize, Deref)]
    #[serde(transparent)]
    struct ArrayIfEmpty(PhpAssocArray<String>);

    #[test]
    fn test_php_assoc_array_populated() {
        assert_de_tokens(
            &ArrayIfEmpty(PhpAssocArray(HashMap::<String, String>::from([(
                "foo".into(),
                "bar".into(),
            )]))),
            &[
                Token::Map { len: Some(1) },
                Token::String("foo"),
                Token::String("bar"),
                Token::MapEnd,
            ],
        );
    }

    #[test]
    fn test_php_assoc_array_empty() {
        assert_de_tokens(
            &ArrayIfEmpty(PhpAssocArray(HashMap::<String, String>::new())),
            &[Token::Seq { len: Some(0) }, Token::SeqEnd],
        );
    }

    #[test]
    fn test_php_assoc_array_errors() {
        assert_de_tokens_error::<ArrayIfEmpty>(
            &[
                Token::Seq { len: Some(1) },
                Token::String("hi"),
                Token::SeqEnd,
            ],
            "sequence must be empty",
        );
        assert_de_tokens_error::<ArrayIfEmpty>(
            &[Token::Seq { len: Some(1) }, Token::U8(42), Token::SeqEnd],
            "sequence must be empty",
        );
    }

    #[test]
    fn test_empty_vec_to_default() {
        assert_de_tokens(
            &EVT {
                foo: HashMap::<String, bool>::new(),
            },
            &[Token::Seq { len: Some(0) }, Token::SeqEnd],
        );
        assert_de_tokens(
            &EVT {
                foo: HashMap::<String, bool>::from([("foo".into(), true)]),
            },
            &[
                Token::Map { len: Some(1) },
                Token::String("foo"),
                Token::Bool(true),
                Token::MapEnd,
            ],
        );
    }

    #[test]
    fn test_empty_vec_to_default_errors() {
        assert_de_tokens_error::<EVT>(
            &[
                Token::Seq { len: Some(1) },
                Token::String("yo"),
                Token::SeqEnd,
            ],
            "got unexpected array",
        );
    }
}

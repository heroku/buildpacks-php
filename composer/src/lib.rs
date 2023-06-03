use derive_more::{Deref, From};
use monostate::MustBe;
use serde::de::{Error, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use serde_with::{formats::PreferOne, serde_as, skip_serializing_none, OneOrMany, TryFromInto};
use std::collections::HashMap;
use url::Url;

use std::fmt;
use std::marker::PhantomData;
use std::path::PathBuf;

#[derive(Clone, Debug, Deref, Serialize, PartialEq, From)]
pub struct PhpAssocArray<T>(HashMap<String, T>);
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
pub struct ComposerRootPackage {
    pub name: Option<String>,
    pub version: Option<String>,
    pub config: Option<Map<String, Value>>,
    pub minimum_stability: Option<ComposerStability>,
    pub prefer_stable: Option<bool>,
    #[serde(flatten)]
    pub package: ComposerBasePackage,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ComposerPackage {
    pub name: String,
    pub version: String,
    #[serde(flatten)]
    pub package: ComposerBasePackage,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ComposerBasePackage {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub abandoned: Option<ComposerPackageAbandoned>,
    pub archive: Option<ComposerPackageArchive>,
    pub authors: Option<Vec<ComposerPackageAuthor>>,
    pub autoload: Option<ComposerPackageAutoload>,
    pub autoload_dev: Option<ComposerPackageAutoload>,
    #[serde_as(as = "Option<OneOrMany<_, PreferOne>>")]
    pub bin: Option<Vec<String>>,
    pub conflict: Option<HashMap<String, String>>,
    pub description: Option<String>,
    pub dist: Option<ComposerPackageDist>,
    pub extra: Option<Value>,
    pub funding: Option<Vec<ComposerPackageFunding>>,
    pub homepage: Option<Url>,
    pub include_path: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
    #[serde_as(as = "Option<OneOrMany<_, PreferOne>>")]
    pub license: Option<Vec<String>>,
    pub minimum_stability: Option<ComposerStability>,
    pub non_feature_branches: Option<Vec<String>>,
    pub prefer_stable: Option<bool>,
    pub provide: Option<HashMap<String, String>>,
    pub readme: Option<PathBuf>,
    pub replace: Option<HashMap<String, String>>,
    pub repositories: Option<Vec<ComposerRepository>>,
    pub require: Option<HashMap<String, String>>,
    pub require_dev: Option<HashMap<String, String>>,
    pub scripts: Option<HashMap<String, Vec<String>>>,
    pub scripts_descriptions: Option<HashMap<String, String>>,
    pub source: Option<ComposerPackageSource>,
    pub support: Option<HashMap<ComposerPackageSupportType, String>>,
    pub suggest: Option<HashMap<String, String>>,
    pub target_dir: Option<String>,
    pub time: Option<String>, // TODO: "Package release date, in 'YYYY-MM-DD', 'YYYY-MM-DD HH:MM:SS' or 'YYYY-MM-DDTHH:MM:SSZ' format.", but in practice it uses DateTime::__construct(), which can parse a lot of formats
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub struct ComposerConfig {
    #[serde(rename = "cache-files-ttl")]
    pub cache_files_ttl: Option<u32>,
    #[serde(rename = "discard-changes")]
    pub discard_changes: Option<bool>,
    #[serde(rename = "allow-plugins")]
    pub allow_plugins: Option<ComposerConfigAllowPlugins>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ComposerConfigAllowPlugins {
    Boolean(bool),
    List(HashMap<String, bool>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ComposerStability {
    Dev = 20,
    Alpha = 15,
    Beta = 10,
    #[serde(alias = "RC")]
    Rc = 5,
    #[default]
    Stable = 0,
}
impl fmt::Display for ComposerStability {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ComposerStability::Dev => write!(f, "dev"),
            ComposerStability::Alpha => write!(f, "alpha"),
            ComposerStability::Beta => write!(f, "beta"),
            ComposerStability::Rc => write!(f, "RC"),
            ComposerStability::Stable => write!(f, "stable"),
        }
    }
}
impl From<ComposerStability> for u8 {
    fn from(value: ComposerStability) -> Self {
        value as u8
    }
}
impl TryFrom<u8> for ComposerStability {
    type Error = String;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(ComposerStability::Stable),
            5 => Ok(ComposerStability::Rc),
            10 => Ok(ComposerStability::Beta),
            15 => Ok(ComposerStability::Alpha),
            20 => Ok(ComposerStability::Dev),
            _ => Err(format!("Invalid stability flag {v}")),
        }
    }
}
impl From<PhpAssocArray<ComposerStability>> for PhpAssocArray<u8> {
    fn from(value: PhpAssocArray<ComposerStability>) -> Self {
        value
            .iter()
            .map(|(k, v)| (k.clone(), v.clone() as u8))
            .collect::<HashMap<String, u8>>()
            .into()
    }
}
impl TryFrom<PhpAssocArray<u8>> for PhpAssocArray<ComposerStability> {
    type Error = String;

    fn try_from(value: PhpAssocArray<u8>) -> Result<Self, Self::Error> {
        let ret = value
            .iter()
            .map(|(k, v)| match ComposerStability::try_from(v.clone()) {
                Ok(value) => Ok((k.clone(), value)),
                Err(e) => Err(e),
            })
            .collect::<Result<HashMap<String, ComposerStability>, _>>();
        match ret {
            Ok(v) => Ok(v.into()),
            Err(e) => Err(e),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum ComposerPackageAbandoned {
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
pub struct ComposerPackageAuthor {
    pub name: String,
    pub email: Option<String>, // TODO: could be EmailAddress, but Composer only warns
    pub homepage: Option<Url>,
    pub role: Option<String>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct ComposerPackageAutoload {
    // map values for the next two can be string or list of strings
    #[serde_as(as = "Option<HashMap<_, OneOrMany<_, PreferOne>>>")]
    pub psr_0: Option<HashMap<String, Vec<String>>>,
    #[serde_as(as = "Option<HashMap<_, OneOrMany<_, PreferOne>>>")]
    pub psr_4: Option<HashMap<String, Vec<String>>>,
    pub classmap: Option<Vec<String>>,
    pub files: Option<Vec<String>>,
    pub exclude_from_classmap: Option<Vec<String>>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ComposerPackageArchive {
    pub name: Option<String>,
    pub exclude: Option<Vec<String>>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub struct ComposerPackageDist {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: Url,
    pub reference: Option<String>,
    pub shasum: Option<String>,
    pub mirrors: Option<Vec<ComposerMirror>>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub struct ComposerPackageSource {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: Url,
    pub reference: Option<String>,
    pub mirrors: Option<Vec<ComposerMirror>>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub struct ComposerMirror {
    pub url: Url,
    pub preferred: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ComposerPackageFunding {
    #[serde(rename = "type")]
    pub kind: String, // default "other"?
    pub url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
#[serde(rename_all = "lowercase")]
#[serde(try_from = "String")]
pub enum ComposerPackageSupportType {
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
pub enum ComposerRepository {
    #[serde(rename_all = "kebab-case")]
    Composer {
        #[serde(rename = "type")]
        kind: MustBe!("composer"),
        url: Url,
        #[serde(rename = "allow_ssl_downgrade")]
        allow_ssl_downgrade: Option<bool>,
        force_lazy_providers: Option<bool>,
        options: Option<Map<String, Value>>,
        canonical: Option<bool>,
        #[serde(flatten)]
        filters: Option<ComposerRepositoryFilters>,
    },
    #[serde(rename_all = "kebab-case")]
    Path {
        #[serde(rename = "type")]
        kind: MustBe!("path"),
        url: PathBuf,
        options: Option<Map<String, Value>>,
        canonical: Option<bool>,
        #[serde(flatten)]
        filters: Option<ComposerRepositoryFilters>,
    },
    #[serde(rename_all = "kebab-case")]
    Package {
        #[serde(rename = "type")]
        kind: MustBe!("package"),
        #[serde_as(as = "OneOrMany<_, PreferOne>")]
        package: Vec<ComposerPackage>,
        canonical: Option<bool>,
        #[serde(flatten)]
        filters: Option<ComposerRepositoryFilters>,
    },
    #[serde(rename_all = "kebab-case")]
    Url {
        #[serde(rename = "type")]
        kind: String,
        url: Url,
        canonical: Option<bool>,
        #[serde(flatten)]
        filters: Option<ComposerRepositoryFilters>,
        #[serde(flatten)]
        extra: HashMap<String, Value>,
    },
    #[serde(rename_all = "kebab-case")]
    Other {
        #[serde(rename = "type")]
        kind: String,
        canonical: Option<bool>,
        #[serde(flatten)]
        filters: Option<ComposerRepositoryFilters>,
        #[serde(flatten)]
        extra: HashMap<String, Value>,
    },
    #[serde(rename_all = "kebab-case")]
    Disabled(HashMap<String, MustBe!(false)>),
}
impl ComposerRepository {
    pub fn from_path_with_options<O>(path: impl Into<PathBuf>, options: O) -> Self
    where
        O: IntoIterator<Item = (String, Value)>,
    {
        Self::Path {
            kind: Default::default(),
            url: path.into(),
            options: Some(Map::from_iter(options)),
            canonical: None,
            filters: None,
        }
    }
}
impl From<Vec<ComposerPackage>> for ComposerRepository {
    fn from(value: Vec<ComposerPackage>) -> Self {
        Self::Package {
            kind: Default::default(),
            package: value,
            canonical: None,
            filters: None,
        }
    }
}
impl FromIterator<ComposerPackage> for ComposerRepository {
    fn from_iter<T: IntoIterator<Item = ComposerPackage>>(iter: T) -> Self {
        ComposerRepository::from(iter.into_iter().collect::<Vec<_>>())
    }
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
pub enum ComposerRepositoryFilters {
    Only(Vec<String>),
    Exclude(Vec<String>),
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "kebab-case")]
pub struct ComposerLock {
    pub content_hash: String, // since 1.0: https://github.com/composer/composer/pull/4140
    pub packages: Vec<ComposerPackage>,
    pub packages_dev: Vec<ComposerPackage>, // could be null before 1.1.0: https://github.com/composer/composer/pull/5224
    pub platform: PhpAssocArray<String>,
    pub platform_dev: PhpAssocArray<String>,
    pub platform_overrides: Option<HashMap<String, String>>, // since 1.0: https://github.com/composer/composer/commit/a57c51e8d78156612e49dec1c54d3184f260f144
    // pub aliases: HashMap<String, ComposerPackage>, // since 1.0: https://github.com/composer/composer/pull/350 - TODO: do we need to handle these?
    pub minimum_stability: ComposerStability, // since 1.0: https://github.com/composer/composer/pull/592
    #[serde_as(as = "TryFromInto<PhpAssocArray<u8>>")]
    pub stability_flags: PhpAssocArray<ComposerStability>, // since 1.0: https://github.com/composer/composer/pull/592
    pub prefer_stable: bool, // since 1.0: https://github.com/composer/composer/pull/3101
    pub prefer_lowest: bool, // since 1.0: https://github.com/composer/composer/pull/3450
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub plugin_api_version: Option<String>, // since 1.10.0: https://github.com/composer/composer/commit/0b9c658bef426a56dc3971e614028ff5078bcd95
}
impl ComposerLock {
    pub fn new(plugin_api_version: Option<String>) -> Self {
        Self {
            content_hash: "".to_string(),
            packages: vec![],
            packages_dev: vec![],
            platform: PhpAssocArray(Default::default()),
            platform_dev: PhpAssocArray(Default::default()),
            platform_overrides: None,
            minimum_stability: ComposerStability::Stable,
            stability_flags: PhpAssocArray(Default::default()),
            prefer_stable: false,
            prefer_lowest: false,
            plugin_api_version,
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

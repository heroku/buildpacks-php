use derive_more::{Deref, From};
use git_url_parse::GitUrl;
use indexmap::IndexMap;
use monostate::MustBe;
use serde::de::{Error, MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use serde_with::{OneOrMany, TryFromInto, formats::PreferOne, serde_as, skip_serializing_none};
use std::fmt;
use std::marker::PhantomData;
use std::path::PathBuf;
use url::Url;

#[derive(Clone, Debug, Default, Deref, From, PartialEq, Serialize)]
pub struct PhpAssocArray<T>(IndexMap<String, T>);
impl<'de, T: Deserialize<'de> + Default> Deserialize<'de> for PhpAssocArray<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct EmptyPhpObjectVisitor<T> {
            marker: PhantomData<T>,
        }

        impl<'de, T> Visitor<'de> for EmptyPhpObjectVisitor<T>
        where
            T: Deserialize<'de> + Default,
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
                    Ok(Some(_)) | Err(_) => Err(Error::custom("sequence must be empty")), // if a T is in there or something else is in there
                    Ok(None) => Ok(PhpAssocArray::default()),
                }
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = IndexMap::<String, T>::new();
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

#[derive(Clone, Debug, PartialEq)]
struct SingleEntryKVMap<T> {
    key: String,
    value: T,
}

impl<'de, T> Deserialize<'de> for SingleEntryKVMap<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SingleEntryKVMapVisitor<T>(PhantomData<T>);
        impl<'de, T> Visitor<'de> for SingleEntryKVMapVisitor<T>
        where
            T: Deserialize<'de>,
        {
            type Value = SingleEntryKVMap<T>;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
                f.write_str("an object with exactly one key-value pair")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                match map.next_entry()? {
                    Some((key, value)) => {
                        // we have one, but do we have more?
                        let next: Option<(String, T)> = map.next_entry()?;
                        match next {
                            Some((_, _)) => Err(Error::invalid_length(2, &self)),
                            None => Ok(SingleEntryKVMap { key, value }),
                        }
                    }
                    None => Err(Error::invalid_length(0, &self)),
                }
            }
        }

        let visitor = SingleEntryKVMapVisitor(PhantomData);

        deserializer.deserialize_map(visitor)
    }
}

impl<T> Serialize for SingleEntryKVMap<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(&self.key, &self.value)?;
        map.end()
    }
}

impl<T> SingleEntryKVMap<T> {
    fn new(key: &str, value: T) -> SingleEntryKVMap<T> {
        Self {
            key: key.into(),
            value,
        }
    }
}

#[skip_serializing_none]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ComposerRootPackage {
    pub name: Option<String>,
    pub version: Option<String>,
    pub config: Option<IndexMap<String, Value>>,
    pub minimum_stability: Option<ComposerStability>,
    pub prefer_stable: Option<bool>,
    #[serde(flatten)]
    pub package: ComposerBasePackage,
}

#[skip_serializing_none]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ComposerPackage {
    pub name: String,
    pub version: String,
    #[serde(flatten)]
    pub package: ComposerBasePackage,
}

#[derive(Clone, Debug, Default, From, Serialize)]
pub struct ComposerRepositories(Vec<ComposerRepository>);

impl<'de> Deserialize<'de> for ComposerRepositories {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ComposerRepositoriesVisitor;

        impl<'de> Visitor<'de> for ComposerRepositoriesVisitor {
            type Value = ComposerRepositories;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an array of repository definitions (or e.g. '{\"packagist.org\": false}' to disable a repo), or an object of repository names as keys and repository definitions (or boolean false to disable a repo) as values")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut values: Vec<ComposerRepository> =
                    Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(value) = seq.next_element::<Value>()? {
                    let repo = serde_json::from_value::<ComposerRepository>(value.clone())
                        .or_else(|_| {
                            serde_json::from_value::<ComposerRepositoryDisablement>(value)
                                .map(ComposerRepository::Disabled)
                        })
                        .map_err(|_| {
                            Error::custom(format!("invalid value at array index {index}, expected repository definition object or disablement using single-key object notation (e.g. `{{\"packagist.org\": false}}`)", index = values.len()))
                        })?;
                    values.push(repo);
                }
                Ok(ComposerRepositories(values))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = Vec::with_capacity(map.size_hint().unwrap_or(0));
                // we fetch entries as serde_json::Value objects so that we can handle the disabled repo (key: $name, value: false) case
                while let Some((key, value)) = map.next_entry::<String, Value>()? {
                    let repo = serde_json::from_value::<ComposerRepository>(value.clone())
                        .or_else(|_| {
                            // try de-serializing as a boolean false
                            serde_json::from_value::<MustBe!(false)>(value)
                                // that's it; we make a "Disabled" repo variant using the key as the name
                                .map(|_| ComposerRepository::Disabled(ComposerRepositoryDisablement::new(&key)))
                        })
                        .map_err(|_| {
                            Error::custom(format!("invalid value at object key '{key}', expected repository definition object or disablement using boolean `false`"))
                        })
                        .and_then(|mut repository| {
                            match repository {
                                ComposerRepository::Composer { ref mut name, .. }
                                | ComposerRepository::Path { ref mut name, .. }
                                | ComposerRepository::Package { ref mut name, .. }
                                | ComposerRepository::Url { ref mut name, .. }
                                | ComposerRepository::Other { ref mut name, .. }
                                => {
                                    if name.is_some() {
                                        Err(Error::custom(format!("invalid value at object key '{key}', repository definition must not have a 'name' field")))
                                    } else {
                                        name.replace(key);
                                        Ok(repository)
                                    }
                                },
                                ComposerRepository::Disabled(_) => Ok(repository)
                            }
                        })?;
                    values.push(repo);
                }
                Ok(ComposerRepositories(values))
            }
        }

        let visitor = ComposerRepositoriesVisitor;

        deserializer.deserialize_any(visitor)
    }
}

#[allow(clippy::iter_without_into_iter)]
impl ComposerRepositories {
    pub fn iter(&self) -> std::slice::Iter<'_, ComposerRepository> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, ComposerRepository> {
        self.0.iter_mut()
    }
}

#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ComposerBasePackage {
    #[serde(rename = "_comment")]
    pub comment: Option<Value>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub abandoned: Option<ComposerPackageAbandoned>,
    pub archive: Option<ComposerPackageArchive>,
    pub authors: Option<Vec<ComposerPackageAuthor>>,
    pub autoload: Option<ComposerPackageAutoload>,
    pub autoload_dev: Option<ComposerPackageAutoload>,
    #[serde_as(as = "Option<OneOrMany<_, PreferOne>>")]
    pub bin: Option<Vec<String>>,
    pub conflict: Option<IndexMap<String, String>>,
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
    pub provide: Option<IndexMap<String, String>>,
    pub readme: Option<PathBuf>,
    pub replace: Option<IndexMap<String, String>>,
    pub repositories: Option<ComposerRepositories>,
    pub require: Option<IndexMap<String, String>>,
    pub require_dev: Option<IndexMap<String, String>>,
    pub scripts_descriptions: Option<IndexMap<String, String>>,
    pub source: Option<ComposerPackageSource>,
    pub support: Option<IndexMap<String, String>>,
    pub suggest: Option<IndexMap<String, String>>,
    pub target_dir: Option<String>,
    pub time: Option<String>, // TODO: "Package release date, in 'YYYY-MM-DD', 'YYYY-MM-DD HH:MM:SS' or 'YYYY-MM-DDTHH:MM:SSZ' format.", but in practice it uses DateTime::__construct(), which can parse a lot of formats
}

#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ComposerConfig {
    #[serde(rename = "cache-files-ttl")]
    pub cache_files_ttl: Option<u32>,
    #[serde(rename = "discard-changes")]
    pub discard_changes: Option<bool>,
    #[serde(rename = "allow-plugins")]
    pub allow_plugins: Option<ComposerConfigAllowPlugins>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum ComposerConfigAllowPlugins {
    Boolean(bool),
    List(IndexMap<String, bool>),
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
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
            .collect::<IndexMap<String, u8>>()
            .into()
    }
}
impl TryFrom<PhpAssocArray<u8>> for PhpAssocArray<ComposerStability> {
    type Error = String;

    fn try_from(value: PhpAssocArray<u8>) -> Result<Self, Self::Error> {
        let ret = value
            .iter()
            .map(|(k, v)| match ComposerStability::try_from(*v) {
                Ok(value) => Ok((k.clone(), value)),
                Err(e) => Err(e),
            })
            .collect::<Result<IndexMap<String, ComposerStability>, _>>();
        match ret {
            Ok(v) => Ok(v.into()),
            Err(e) => Err(e),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ComposerPackageAuthor {
    pub name: String,
    pub email: Option<String>, // TODO: could be EmailAddress, but Composer only warns
    pub homepage: Option<Url>,
    pub role: Option<String>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ComposerPackageAutoload {
    // map values for the next two can be string or list of strings
    #[serde_as(as = "Option<IndexMap<_, OneOrMany<_, PreferOne>>>")]
    pub psr_0: Option<IndexMap<String, Vec<String>>>,
    #[serde_as(as = "Option<IndexMap<_, OneOrMany<_, PreferOne>>>")]
    pub psr_4: Option<IndexMap<String, Vec<String>>>,
    pub classmap: Option<Vec<String>>,
    pub files: Option<Vec<String>>,
    pub exclude_from_classmap: Option<Vec<String>>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ComposerPackageArchive {
    pub name: Option<String>,
    pub exclude: Option<Vec<String>>,
}

// like ComposerRepository, we must declare this as untagged since we're relying on MustBe! for one variant
#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ComposerPackageDist {
    #[serde(rename_all = "kebab-case")]
    Path {
        #[serde(rename = "type")]
        kind: MustBe!("path"),
        url: PathBuf,
        reference: Option<String>,
        shasum: Option<String>,
    },
    #[serde(rename_all = "kebab-case")]
    Url {
        #[serde(rename = "type")]
        kind: String,
        url: Url,
        reference: Option<String>,
        shasum: Option<String>,
        mirrors: Option<Vec<ComposerMirror<Url>>>,
    },
}

#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ComposerPackageSource {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: ComposerUrlOrScpLikeOrPathUrl,
    pub reference: Option<String>,
    pub mirrors: Option<Vec<ComposerMirror<ComposerUrlOrScpLikeOrPathUrl>>>,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ComposerMirror<T> {
    pub url: T,
    pub preferred: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ComposerPackageFunding {
    #[serde(rename = "type")]
    pub kind: String, // default "other"?
    pub url: String,
}

// this is not actually untagged, but we need to mix two entry types:
// 1) internally tagged via "type" for "real" repository types
// 2) { "name": false } (for repositories array) or bool false (for repositories object) to disable a repo (e.g. { "packagist.org": false }, not uncommon for various reasons)
// Tagged enums can have #[serde(other)] only for a unit type: https://github.com/serde-rs/serde/issues/912
// A workaround with tags declared on separate structs (https://stackoverflow.com/a/61219284/162354) doesn't work as explained in https://stackoverflow.com/a/74544853/162354
// The solution is to rely on monostate's MustBe!
#[serde_as]
#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ComposerRepository {
    #[serde(rename_all = "kebab-case")]
    Composer {
        name: Option<String>,
        #[serde(rename = "type")]
        kind: MustBe!("composer"),
        url: ComposerUrlOrPathUrl, // can also be a relative path
        #[serde(rename = "allow_ssl_downgrade")]
        allow_ssl_downgrade: Option<bool>,
        force_lazy_providers: Option<bool>,
        options: Option<IndexMap<String, Value>>,
        canonical: Option<bool>,
        #[serde(flatten)]
        filters: Option<ComposerRepositoryFilters>,
    },
    #[serde(rename_all = "kebab-case")]
    Path {
        name: Option<String>,
        #[serde(rename = "type")]
        kind: MustBe!("path"),
        url: PathBuf,
        options: Option<IndexMap<String, Value>>,
        canonical: Option<bool>,
        #[serde(flatten)]
        filters: Option<ComposerRepositoryFilters>,
    },
    #[serde(rename_all = "kebab-case")]
    Package {
        name: Option<String>,
        #[serde(rename = "type")]
        kind: MustBe!("package"),
        #[serde_as(as = "OneOrMany<_, PreferOne>")]
        package: Vec<ComposerPackage>,
        canonical: Option<bool>,
        #[serde(flatten)]
        filters: Option<ComposerRepositoryFilters>,
    },
    // any other repo type that has a URL field
    #[serde(rename_all = "kebab-case")]
    Url {
        name: Option<String>,
        #[serde(rename = "type")]
        kind: String,
        url: ComposerUrlOrScpLikeOrPathUrl, // can be an SCP style SSH "URL" or relative path, too
        canonical: Option<bool>,
        #[serde(flatten)]
        filters: Option<ComposerRepositoryFilters>,
        #[serde(flatten)]
        extra: IndexMap<String, Value>,
    },
    #[serde(rename_all = "kebab-case")]
    Other {
        name: Option<String>,
        #[serde(rename = "type")]
        kind: String,
        canonical: Option<bool>,
        #[serde(flatten)]
        filters: Option<ComposerRepositoryFilters>,
        #[serde(flatten)]
        extra: IndexMap<String, Value>,
    },
    // we never want this to be (automatically) deserialized
    // our Deserialize implementation for the ComposerRepositories container takes care of this case
    // otherwise, having a map or seq entry in the JSON with just a string would deserialize to this variant
    #[serde(skip_deserializing)]
    Disabled(ComposerRepositoryDisablement),
}

impl ComposerRepository {
    pub fn from_path_with_options(
        path: impl Into<PathBuf>,
        options: Option<IndexMap<String, Value>>,
    ) -> Self {
        Self::Path {
            name: None,
            #[allow(clippy::default_trait_access)]
            kind: Default::default(),
            url: path.into(),
            options,
            canonical: None,
            filters: None,
        }
    }
}
impl From<Vec<ComposerPackage>> for ComposerRepository {
    fn from(value: Vec<ComposerPackage>) -> Self {
        Self::Package {
            name: None,
            #[allow(clippy::default_trait_access)]
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
impl From<ComposerRepositoryDisablement> for ComposerRepository {
    fn from(value: ComposerRepositoryDisablement) -> Self {
        Self::Disabled(value)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct ComposerRepositoryDisablement(SingleEntryKVMap<MustBe!(false)>);
impl ComposerRepositoryDisablement {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self(SingleEntryKVMap::new(name, MustBe!(false)))
    }
}
impl From<&str> for ComposerRepositoryDisablement {
    fn from(value: &str) -> Self {
        ComposerRepositoryDisablement::new(value)
    }
}
impl AsRef<str> for ComposerRepositoryDisablement {
    fn as_ref(&self) -> &str {
        &self.0.key
    }
}
impl fmt::Display for ComposerRepositoryDisablement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", &self.as_ref())
    }
}

#[skip_serializing_none]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ComposerRepositoryFilters {
    Only(Vec<String>),
    Exclude(Vec<String>),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(from = "String")]
#[serde(into = "String")]
pub enum ComposerUrlOrPathUrl {
    Url(Url),
    Path(PathBuf),
}

impl From<ComposerUrlOrPathUrl> for String {
    fn from(val: ComposerUrlOrPathUrl) -> Self {
        match val {
            ComposerUrlOrPathUrl::Url(v) => v.into(),
            ComposerUrlOrPathUrl::Path(v) => format!("{}", v.display()),
        }
    }
}

impl From<String> for ComposerUrlOrPathUrl {
    fn from(value: String) -> Self {
        Url::parse(&value).map_or_else(|_| Self::Path(PathBuf::from(&value)), Self::Url)
    }
}

// For some sources (and their mirrors) such as 'git', three kinds of URLs are allowed:
// 1) URL style (e.g. https://github.com/foo/bar)
// 2) SCP style SSH (e.g. git@github.com:foo/bar.git)
// 3) local filesystem path
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(from = "String")]
#[serde(into = "String")]
pub enum ComposerUrlOrScpLikeOrPathUrl {
    Url(Url),
    ScpLike(GitUrl),
    Path(PathBuf),
}

impl From<ComposerUrlOrScpLikeOrPathUrl> for String {
    fn from(val: ComposerUrlOrScpLikeOrPathUrl) -> Self {
        match val {
            ComposerUrlOrScpLikeOrPathUrl::Url(v) => v.into(),
            ComposerUrlOrScpLikeOrPathUrl::ScpLike(v) => format!("{v}"),
            ComposerUrlOrScpLikeOrPathUrl::Path(v) => format!("{}", v.display()),
        }
    }
}

impl From<String> for ComposerUrlOrScpLikeOrPathUrl {
    fn from(value: String) -> Self {
        // try and parse as a regular Url first
        // reason is that Url handles e.g. 'svn+ssh://...' correctly
        // GitUrl really is only for the 'git@github.com:foo/bar.git' SCP style cases
        Url::parse(&value).map_or_else(
            |_| {
                GitUrl::parse(&value).map_or_else(
                    |_| Self::Path(PathBuf::from(&value)),
                    |git_url| {
                        match &git_url.scheme() {
                            // it's a path with no leading file:// scheme
                            // git-url-parse accepts those
                            // in that case, we return a PathBuf instead of a Url
                            Some("file") => Self::Path(PathBuf::from(git_url.path())),
                            //  it's an SSH style URL, "normalize" it to url::Url
                            _ => Self::ScpLike(git_url),
                        }
                    },
                )
            },
            Self::Url,
        )
    }
}

#[serde_as]
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ComposerLock {
    pub content_hash: String, // since 1.0: https://github.com/composer/composer/pull/4140
    pub packages: Vec<ComposerPackage>,
    pub packages_dev: Vec<ComposerPackage>, // could be null before 1.1.0: https://github.com/composer/composer/pull/5224
    pub platform: PhpAssocArray<String>,
    pub platform_dev: PhpAssocArray<String>,
    pub platform_overrides: Option<IndexMap<String, String>>, // since 1.0: https://github.com/composer/composer/commit/a57c51e8d78156612e49dec1c54d3184f260f144
    // pub aliases: IndexMap<String, ComposerPackage>, // since 1.0: https://github.com/composer/composer/pull/350 - TODO: do we need to handle these?
    pub minimum_stability: ComposerStability, // since 1.0: https://github.com/composer/composer/pull/592
    #[serde_as(as = "TryFromInto<PhpAssocArray<u8>>")]
    pub stability_flags: PhpAssocArray<ComposerStability>, // since 1.0: https://github.com/composer/composer/pull/592
    pub prefer_stable: bool, // since 1.0: https://github.com/composer/composer/pull/3101
    pub prefer_lowest: bool, // since 1.0: https://github.com/composer/composer/pull/3450
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub plugin_api_version: Option<String>, // since 1.10.0: https://github.com/composer/composer/commit/0b9c658bef426a56dc3971e614028ff5078bcd95
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use std::fs;

    use serde_test::{Token, assert_de_tokens, assert_de_tokens_error, assert_tokens};

    #[derive(Debug, Deref, Deserialize, PartialEq, Serialize)]
    #[serde(transparent)]
    struct ArrayIfEmpty(PhpAssocArray<String>);

    #[test]
    fn test_php_assoc_array_populated() {
        assert_de_tokens(
            &ArrayIfEmpty(PhpAssocArray(IndexMap::from([(
                "foo".to_string(),
                "bar".to_string(),
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
            &ArrayIfEmpty(PhpAssocArray::default()),
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

    #[derive(Debug, Deref, Deserialize, PartialEq, Serialize)]
    #[serde(transparent)]
    struct SingleEntryObject(SingleEntryKVMap<String>);

    #[test]
    fn test_single_entry_kv_map() {
        assert_tokens(
            &SingleEntryObject(SingleEntryKVMap::new("foo", "bar".to_string())),
            &[
                Token::Map { len: Some(1) },
                Token::String("foo"),
                Token::String("bar"),
                Token::MapEnd,
            ],
        );
    }

    #[test]
    fn test_single_entry_kv_map_errors() {
        assert_de_tokens_error::<SingleEntryObject>(
            &[Token::Map { len: Some(0) }, Token::MapEnd],
            "invalid length 0, expected an object with exactly one key-value pair",
        );
        assert_de_tokens_error::<SingleEntryObject>(
            &[
                Token::Map { len: Some(2) },
                Token::String("one"),
                Token::String("Mississippi"),
                Token::String("two"),
                Token::String("Mississippi"),
                Token::MapEnd,
            ],
            "invalid length 2, expected an object with exactly one key-value pair",
        );
    }

    #[test]
    fn test_repository_disablement_errors() {
        assert_de_tokens_error::<ComposerRepositories>(
            &[
                Token::Map { len: Some(1) },
                Token::String("packagist.org"),
                Token::Bool(true),
                Token::MapEnd,
            ],
            "invalid value at object key 'packagist.org', expected repository definition object or disablement using boolean `false`",
        );
        assert_de_tokens_error::<ComposerRepositories>(
            &[
                Token::Seq { len: Some(1) },
                Token::Map { len: Some(1) },
                Token::String("packagist.org"),
                Token::Bool(true),
                Token::MapEnd,
                Token::SeqEnd,
            ],
            "invalid value at array index 0, expected repository definition object or disablement using single-key object notation (e.g. `{\"packagist.org\": false}`)",
        );
    }

    #[test]
    fn test_composer_url_or_scplike_or_path_url() {
        for case in ["file:///foo/bar", "https://github.com/foo/bar.git"] {
            let parsed = ComposerUrlOrScpLikeOrPathUrl::from(case.to_string());
            assert!(matches!(parsed, ComposerUrlOrScpLikeOrPathUrl::Url(_)));
            assert_eq!(String::from(parsed), case.to_string());
        }
        for case in ["git@github.com:foo/bar.git", "git@server.com:~/foo/bar.git"] {
            let parsed = ComposerUrlOrScpLikeOrPathUrl::from(case.to_string());
            assert!(matches!(parsed, ComposerUrlOrScpLikeOrPathUrl::ScpLike(_)));
            assert_eq!(String::from(parsed), case.to_string());
        }
        for case in ["/foo/bar", "../foo/bar", "foo/bar", "./foo@bar:baz"] {
            let parsed = ComposerUrlOrScpLikeOrPathUrl::from(case.to_string());
            assert!(matches!(parsed, ComposerUrlOrScpLikeOrPathUrl::Path(_)));
            assert_eq!(String::from(parsed), case.to_string());
        }
    }

    #[test]
    fn test_composer_url_or_path_url() {
        for case in ["file:///foo/bar", "https://github.com/foo/bar.git"] {
            let parsed = ComposerUrlOrPathUrl::from(case.to_string());
            assert!(matches!(parsed, ComposerUrlOrPathUrl::Url(_)));
            assert_eq!(String::from(parsed), case.to_string());
        }
        for case in [
            "/foo/bar",
            "../foo/bar",
            "foo/bar",
            "./foo@bar:baz",
            "git@github.com:foo/bar.git",
            "git@server.com:~/foo/bar.git",
        ] {
            let parsed = ComposerUrlOrPathUrl::from(case.to_string());
            assert!(matches!(parsed, ComposerUrlOrPathUrl::Path(_)));
            assert_eq!(String::from(parsed), case.to_string());
        }
    }

    #[rstest]
    fn test_composer_json(
        #[files("tests/fixtures/*.json")]
        #[exclude(r"\.xfail\.json$")]
        path: PathBuf,
    ) {
        let composer_json = std::io::BufReader::new(fs::File::open(&path).unwrap());
        let package: ComposerRootPackage = serde_json::from_reader(composer_json).unwrap();
        // The test cases may have special testing related instructions in the _comment field
        if let Some(Value::Array(ref comments)) = package.package.comment {
            comments
                .iter()
                .filter_map(|comment| match comment {
                    Value::Object(comment) => Some(comment),
                    _ => None,
                })
                .flat_map(std::iter::IntoIterator::into_iter)
                .for_each(|comment| match comment {
                    (op, Value::String(details)) if op == "compare-deserialized" => {
                        // We are supposed to compare the deserialized form to that of another file
                        // This is useful for testing e.g. normalizations, such as for the repositories notations
                        let mut their_path = path.clone();
                        their_path.set_file_name(details);
                        let mut ours = package.clone();
                        let mut theirs: ComposerRootPackage = serde_json::from_reader(
                            std::io::BufReader::new(fs::File::open(their_path).unwrap()),
                        )
                        .unwrap();
                        // The _comment fields obviously differ, so we discard them before comparing
                        ours.package.comment = None;
                        theirs.package.comment = None;
                        assert_json_diff::assert_json_eq!(ours, theirs);
                    }
                    _ => panic!("unexpected magic testing comment instruction"),
                });
        }
    }

    #[allow(clippy::should_panic_without_expect)]
    #[should_panic]
    #[rstest]
    fn test_broken_composer_json(#[files("tests/fixtures/*.xfail.json")] path: PathBuf) {
        let composer_json = fs::read(&path).unwrap();
        serde_json::from_slice::<ComposerRootPackage>(&composer_json).unwrap();
    }

    #[rstest]
    fn test_composer_lock(#[files("tests/fixtures/*.lock")] path: PathBuf) {
        let composer_lock = fs::read(&path).unwrap();
        serde_json::from_slice::<ComposerLock>(&composer_lock).unwrap();
    }
}

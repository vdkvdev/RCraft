use std::collections::HashMap;
use crate::models::{MinecraftVersion, Profile, Section, Theme, ModSearchResult};
use crate::settings::Settings;

#[derive(Debug)]
pub enum AppMsg {
    LaunchProfile(String),
    DeleteProfile(String),
    UsernameChanged(String),
    VersionSelected(String),
    RamChanged(u32),
    ToggleFabric(bool),
    SaveProfile,
    // CancelCreate removed
    VersionsLoaded(Result<Vec<MinecraftVersion>, String>),
    ProfilesLoaded(Result<HashMap<String, Profile>, String>),
    // DownloadCompleted removed
    // DownloadStarted(String) removed
    DownloadProgress(f64, String),
    GameStarted,
    LaunchCompleted,
    NavigateToSection(Section),
    BackToMainMenu,
    // UpdateDownloadDots removed
    OpenMinecraftFolder,
    // ShowAboutWindow removed
    ThemeSelected(Theme),
    ToggleHideLogs(bool),
    ToggleHideMods(bool),
    ToggleSidebar,
    Log(String),


    Error(String),
    RequestDeleteProfile(String),
    SettingsLoaded(Settings),
    SessionEnded(String, u64),
    // ColorsLoaded removed
    RefreshInstalledMods,
    SelectModProfile(String),
    // Modrinth Messages
    SearchMods(String),
    ModsSearched(Result<Vec<ModSearchResult>, String>),
    InstallMod(String), // Project ID
    UninstallMod(String), // Filename
    DownloadModIcon(String, String), // Project ID, URL
    ModIconDownloaded(String, String), // project_id, path
    ProcessIconQueue,
    ModActionButtonClicked(String), // project_id (Toggle Install/Uninstall)
    ModInstallFinished(String, ()), // project_id, success (bool unused)
    ModUninstallFinished(String), // project_id
    RegisterInstalledMod(String, String), // project_id, filename
    ShowToast(String),
    ClearPendingSelection,
    ModDropdownUpdated,
    OpenModrinthPage(String),
    ShowJavaDialog(u32),
    JavaDownloadConfirmed,
    JavaDownloadCancelled,
    InstallJavaAndLaunch,
}

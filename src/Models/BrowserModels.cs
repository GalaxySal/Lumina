using System.Text.Json.Serialization;

namespace tauri_browser.Models
{
    public class TabInfo
    {
        public string Id { get; set; } = "tab-" + Guid.NewGuid().ToString();
        public string Title { get; set; } = "Yeni Sekme";
        public string Url { get; set; } = "about:blank";
        public string? FaviconUrl { get; set; }
        public bool IsLoading { get; set; }
        public bool CanGoBack { get; set; }
        public bool CanGoForward { get; set; }
        public bool IsPwaAvailable { get; set; } = false;
        public uint BlockedAdsCount { get; set; } = 0;
        public bool IsPinned { get; set; } = false;
        [JsonPropertyName("is_incognito")]
        public bool IsIncognito { get; set; } = false;
        [JsonPropertyName("zoom_level")]
        public int ZoomLevel { get; set; } = 100;
    }

    public class CreateTabArgs
    {
        public string Url { get; set; } = "lumina://home";
        public bool Background { get; set; } = false;
    }

    public class HistoryItem 
    { 
        public string Url { get; set; } = string.Empty; 
        public string Title { get; set; } = string.Empty; 
        [JsonPropertyName("visit_count")]
        public long VisitCount { get; set; }
        [JsonPropertyName("last_visit")]
        public long LastVisit { get; set; }
    }

    public class FavoriteItem 
    { 
        public string Url { get; set; } = string.Empty; 
        public string Title { get; set; } = string.Empty; 
    }

    public class DownloadItem
    {
        [JsonPropertyName("url")]
        public string Url { get; set; } = string.Empty;
        
        [JsonPropertyName("file_name")]
        public string FileName { get; set; } = string.Empty;
        
        [JsonPropertyName("status")]
        public string Status { get; set; } = "Downloading";
        
        [JsonPropertyName("path")]
        public string Path { get; set; } = string.Empty;
        
        [JsonPropertyName("downloaded_size")]
        public ulong Progress { get; set; } = 0;
        
        [JsonPropertyName("total_size")]
        public ulong Total { get; set; } = 0;
    }

    public class DownloadStartedPayload
    {
        [JsonPropertyName("url")]
        public string Url { get; set; } = string.Empty;
        [JsonPropertyName("file_name")]
        public string FileName { get; set; } = string.Empty;
    }

    public class DownloadProgressPayload
    {
        [JsonPropertyName("url")]
        public string Url { get; set; } = string.Empty;
        [JsonPropertyName("progress")]
        public ulong Progress { get; set; } = 0;
        [JsonPropertyName("total")]
        public ulong Total { get; set; } = 0;
    }

    public class DownloadFinishedPayload
    {
        [JsonPropertyName("url")]
        public string Url { get; set; } = string.Empty;
        [JsonPropertyName("success")]
        public bool Success { get; set; } = false;
        [JsonPropertyName("path")]
        public string? Path { get; set; } = string.Empty;
    }

    public class TabCreatedPayload
    {
        public string Label { get; set; } = string.Empty;
        public string Url { get; set; } = string.Empty;
    }

    public class TabUpdatedPayload
    {
        [JsonPropertyName("label")]
        public string Label { get; set; } = string.Empty;
        
        [JsonPropertyName("title")]
        public string? Title { get; set; }
        
        [JsonPropertyName("favicon")]
        public string? Favicon { get; set; }
    }

    public class AdblockStatsPayload
    {
        [JsonPropertyName("label")]
        public string Label { get; set; } = string.Empty;
        
        [JsonPropertyName("blocked_count")]
        public uint BlockedCount { get; set; }
    }
    
    public class AppSettings
    {
        public string Homepage { get; set; } = "https://www.google.com";
        public string SearchEngine { get; set; } = "google";
        public string Theme { get; set; } = "dark";
        public string AccentColor { get; set; } = "#3b82f6";
        public bool VerticalTabs { get; set; } = false;
        public bool RoundedCorners { get; set; } = true;
        public bool EnableCookies { get; set; } = true;
        public bool EnableFormData { get; set; } = true;
        public long CookieExpiresDays { get; set; } = 365;
    }
}

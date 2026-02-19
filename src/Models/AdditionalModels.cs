using System.Text.Json.Serialization;

namespace tauri_browser.Models
{
    public class CookieItem
    {
        public string Domain { get; set; } = string.Empty;
        public string Name { get; set; } = string.Empty;
        public string Value { get; set; } = string.Empty;
        public long? Expires { get; set; }
        public string Path { get; set; } = "/";
        public bool Secure { get; set; }
        [JsonPropertyName("http_only")]
        public bool HttpOnly { get; set; }
    }

    public class FindInPageRequest
    {
        public string Query { get; set; } = string.Empty;
        [JsonPropertyName("case_sensitive")]
        public bool CaseSensitive { get; set; } = false;
    }

    public class PrintToPdfRequest
    {
        [JsonPropertyName("label")]
        public string TabLabel { get; set; } = string.Empty;
        public string? Filename { get; set; }
    }

    public class WebStorageItem
    {
        public string Domain { get; set; } = string.Empty;
        public string Key { get; set; } = string.Empty;
        public string Value { get; set; } = string.Empty;
        [JsonPropertyName("storage_type")]
        public string StorageType { get; set; } = "localStorage";
    }

    public class ZoomLevelPayload
    {
        public string? Domain { get; set; }
        [JsonPropertyName("zoom_level")]
        public int ZoomLevel { get; set; } = 100;
    }

    public class SSLWarningPayload
    {
        public string Url { get; set; } = string.Empty;
        [JsonPropertyName("cert_error")]
        public string CertError { get; set; } = string.Empty;
    }

    public class FormDataSuggestion
    {
        [JsonPropertyName("field_name")]
        public string FieldName { get; set; } = string.Empty;
        public List<string> Suggestions { get; set; } = new();
    }
}

using System.Text.Json.Serialization;

namespace tauri_browser.Models
{
    public class StoreItem
    {
        [JsonPropertyName("id")]
        public string Id { get; set; } = "";

        [JsonPropertyName("title")]
        public string Title { get; set; } = "";

        [JsonPropertyName("author")]
        public string Author { get; set; } = "";

        [JsonPropertyName("description")]
        public string Description { get; set; } = "";

        [JsonPropertyName("icon")]
        public string Icon { get; set; } = "";

        [JsonPropertyName("version")]
        public string Version { get; set; } = "";

        [JsonPropertyName("tags")]
        public List<string> Tags { get; set; } = [];

        [JsonPropertyName("verified")]
        public bool Verified { get; set; }

        [JsonPropertyName("installed")]
        public bool Installed { get; set; }

        [JsonPropertyName("comingSoon")]
        public bool ComingSoon { get; set; }
    }

    public class ToastPayload
    {
        [JsonPropertyName("message")]
        public string Message { get; set; } = "";

        [JsonPropertyName("level")]
        public string Level { get; set; } = "info";
    }
}

using System.Text.Json.Serialization;

namespace tauri_browser.Models
{
    public class SuggestionItem
    {
        [JsonPropertyName("icon")]
        public string Icon { get; set; } = "";

        [JsonPropertyName("title")]
        public string Title { get; set; } = "";

        [JsonPropertyName("url")]
        public string Url { get; set; } = "";
    }

    public class OmniboxResponse
    {
        [JsonPropertyName("suggestions")]
        public List<SuggestionItem> Suggestions { get; set; } = [];
    }
}

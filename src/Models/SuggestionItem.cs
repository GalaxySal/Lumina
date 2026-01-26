using System.Text.Json.Serialization;

namespace tauri_browser.Models
{
    public class SuggestionItem
    {
        [JsonPropertyName("title")]
        public string Title { get; set; } = "";
        
        [JsonPropertyName("url")]
        public string Url { get; set; } = "";
        
        [JsonPropertyName("icon")]
        public string Icon { get; set; } = "";
        
        [JsonPropertyName("type")]
        public string Type { get; set; } = "";
    }
}

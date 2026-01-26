using Microsoft.JSInterop;

namespace tauri_browser.Services
{
    public class TauriService
    {
        private readonly IJSRuntime _jsRuntime;

        public TauriService(IJSRuntime jsRuntime)
        {
            _jsRuntime = jsRuntime;
        }

        public async Task<T> InvokeAsync<T>(string command, object? args = null)
        {
            try
            {
                return await _jsRuntime.InvokeAsync<T>("lumina.invoke", command, args);
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Tauri Invoke Error [{command}]: {ex.Message}");
                throw;
            }
        }

        public async Task InvokeVoidAsync(string command, object? args = null)
        {
            try
            {
                await _jsRuntime.InvokeVoidAsync("__TAURI__.core.invoke", command, args);
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Tauri Invoke Error [{command}]: {ex.Message}");
                throw;
            }
        }
    }
}

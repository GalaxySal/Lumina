using Microsoft.JSInterop;

namespace tauri_browser.Services
{
    public class TauriService(IJSRuntime jsRuntime)
    {
        private readonly IJSRuntime _jsRuntime = jsRuntime;

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

        public async Task Listen<T>(string eventName, Action<T> handler)
        {
            try 
            {
                var helper = DotNetObjectReference.Create(new EventCallbackHelper<T>(handler));
                await _jsRuntime.InvokeVoidAsync("lumina.listen", eventName, helper);
            }
            catch (Exception ex)
            {
                Console.WriteLine($"Tauri Listen Error [{eventName}]: {ex.Message}");
            }
        }
    }

    public class EventCallbackHelper<T>(Action<T> action)
    {
        private readonly Action<T> _action = action;
        
        [JSInvokable]
        public void OnEvent(T payload) => _action(payload);
    }
}

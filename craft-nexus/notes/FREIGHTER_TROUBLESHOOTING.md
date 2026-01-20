# Freighter Wallet Connection Troubleshooting

If you're having trouble connecting your Freighter wallet, follow these steps:

## ✅ Quick Fixes

### 1. **Refresh the Page**
After installing or reinstalling Freighter, you **must refresh the page** for it to be detected.

### 2. **Check Extension Status**
- Open Chrome/Edge extensions: `chrome://extensions/` or `edge://extensions/`
- Make sure Freighter is **enabled**
- Make sure Freighter is **not in "Allow in private browsing only"** mode

### 3. **Unlock Your Wallet**
- Open Freighter extension
- Make sure your wallet is unlocked (enter your password if needed)

### 4. **Clear Browser Cache**
- Hard refresh: `Ctrl+Shift+R` (Windows) or `Cmd+Shift+R` (Mac)
- Or clear cache and reload

## 🔍 Debug Steps

### Check Console for Errors
1. Open Developer Tools: `F12` or right-click → Inspect
2. Go to Console tab
3. Look for messages starting with "Freighter:"
4. Common messages:
   - `Freighter: Extension detected` ✅ Good
   - `Freighter: Extension not detected` ❌ Extension not installed/loading
   - `Freighter: API check result: { isConnected: true }` ✅ Ready to connect

### Verify Extension Installation
1. Check if Freighter appears in your extensions list
2. Try clicking the Freighter icon in your browser toolbar
3. If it doesn't open, the extension may not be properly installed

## 🔧 Advanced Troubleshooting

### If Extension is Installed but Not Detected:

1. **Check Extension Permissions**
   - Go to `chrome://extensions/`
   - Find Freighter
   - Click "Details"
   - Make sure all permissions are granted

2. **Reinstall Extension**
   - Remove Freighter completely
   - Close and reopen browser
   - Reinstall from [freighter.app](https://freighter.app)
   - **Refresh this page**

3. **Check Browser Compatibility**
   - Freighter works on: Chrome, Edge, Brave, Opera
   - Does NOT work on: Firefox, Safari (yet)

4. **Try Incognito Mode**
   - Some extensions may be disabled in normal mode
   - Test in incognito (if extension is allowed there)

### Common Error Messages

**"Freighter extension not installed"**
- Solution: Install from freighter.app and refresh page

**"Request access timeout"**
- Solution: Make sure Freighter is unlocked, then try again

**"Failed to get wallet address"**
- Solution: Unlock your Freighter wallet first

**"API not responding"**
- Solution: Refresh the page, wait a few seconds, try again

## 🆘 Still Not Working?

1. Check the browser console for detailed error messages
2. Make sure you're using a supported browser (Chrome/Edge/Brave)
3. Try disabling other extensions that might interfere
4. Check Freighter's official docs: https://docs.freighter.app

## 💡 Pro Tips

- **Always refresh the page** after installing/updating Freighter
- Keep Freighter **unlocked** when connecting
- Use **Chrome or Edge** for best compatibility
- Check console logs for detailed debugging info

---

**Need More Help?** Check the [Freighter Documentation](https://docs.freighter.app) or open an issue on GitHub.

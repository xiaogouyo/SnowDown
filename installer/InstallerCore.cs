using System.Linq;
using System;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Security.Cryptography;
using System.Collections.Generic;
using System.Text;
using Microsoft.Win32;

namespace SnowDown.Setup
{
    internal static class InstallerCore
    {
        internal const string ManifestName = ".snowdown-program-files.v1";
        internal static string LogPath;
        internal static void Log(string message)
        {
            try
            {
                if (LogPath == null)
                {
                    string folder = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "SnowDown", "Logs");
                    Directory.CreateDirectory(folder);
                    LogPath = Path.Combine(folder, "setup-" + DateTime.Now.ToString("yyyyMMdd-HHmmss-fff") + "-" + Process.GetCurrentProcess().Id + ".log");
                }
                File.AppendAllText(LogPath, DateTime.Now.ToString("O") + " " + message + Environment.NewLine, Encoding.UTF8);
            }
            catch (IOException) { } catch (UnauthorizedAccessException) { }
        }

        internal static void CheckOverwrite(string directory)
        {
            // Check every file the payload will replace BEFORE copying any bytes.
            // This also catches lingering media/browser processes and read-only files.
            string manifest;
            using (Stream source = Assembly.GetExecutingAssembly().GetManifestResourceStream("SnowDown.Manifest"))
            using (StreamReader reader = new StreamReader(source)) manifest = reader.ReadToEnd();
            foreach (string relative in ParseManifest(manifest).Keys.Concat(new[] { "uninstall.exe" }))
            {
                string path = OwnedPath(directory, relative);
                if (path == null) throw new IOException("安装目录含有链接或无效的程序路径，请选择其他独立文件夹。");
                if (!File.Exists(path)) continue;
                for (int attempt = 0; ; attempt++)
                {
                    try { using (FileStream file = new FileStream(path, FileMode.Open, FileAccess.ReadWrite, FileShare.None)) { } break; }
                    catch (IOException) { if (attempt >= 20) { Log("File busy: " + relative); throw new IOException("程序文件仍被占用：" + relative + "。请退出 SnowDown 和浏览器后重试。"); } System.Threading.Thread.Sleep(250); }
                    catch (UnauthorizedAccessException) { Log("File access denied: " + relative); throw new IOException("无法覆盖程序文件：" + relative + "。请检查安装目录权限或只读属性。"); }
                }
            }
        }
        internal static bool AutomaticUpdate(string[] args)
        {
            bool automatic = Array.Exists(args, argument => argument == "--auto-update");
            if (automatic && (UpdateDirectory(args) == null || !Array.Exists(args, argument => argument.StartsWith("--wait-pid=", StringComparison.Ordinal))))
                throw new ArgumentException("自动更新缺少原安装目录或待退出进程。");
            return automatic;
        }

        private static string OwnedPath(string directory, string relative)
        {
            relative = relative.Replace('/', '\\');
            if (Path.IsPathRooted(relative) || relative.IndexOf(':') >= 0 || relative.Contains("..")) return null;
            string[] pieces = relative.Split('\\');
            string name = pieces[pieces.Length - 1];
            bool rootFile = pieces.Length == 1 && (name.Equals("snowdown.exe", StringComparison.OrdinalIgnoreCase) || name.Equals("uninstall.exe", StringComparison.OrdinalIgnoreCase) || name.Equals("snowdown.pdb", StringComparison.OrdinalIgnoreCase) || name.Equals("snowdown.dll", StringComparison.OrdinalIgnoreCase));
            string extension = Path.GetExtension(name).ToLowerInvariant();
            bool resource = pieces.Length >= 2 && pieces[0].Equals("resources", StringComparison.OrdinalIgnoreCase)
                && (extension == ".exe" || extension == ".dll" || extension == ".pak" || extension == ".pdb" || (pieces.Length >= 3 && pieces[1] == "browser-extension" && new[]{".js",".json",".html",".css",".png",".txt"}.Contains(extension)) || name == "ffmpeg-build.json" || name == "NOTICE.txt" || name == "FFMPEG-LICENSE.txt");
            if (!rootFile && !resource) return null;
            string lower = name.ToLowerInvariant();
            if (lower.Contains("cookie") || lower.Contains("session") || lower.Contains("login") || lower.Contains("setting")) return null;
            string root = NormalizePath(directory);
            string path = root;
            if ((File.GetAttributes(root) & FileAttributes.ReparsePoint) != 0) return null;
            foreach (string piece in pieces)
            {
                path = Path.Combine(path, piece);
                if ((File.Exists(path) || Directory.Exists(path)) && (File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0) return null;
            }
            return path;
        }

        internal static Dictionary<string,string> ParseManifest(string text)
        {
            Dictionary<string,string> result = new Dictionary<string,string>(StringComparer.OrdinalIgnoreCase);
            if (text == null || text.Length > 128 * 1024 || !text.StartsWith("SnowDown-owned-files-v1\n", StringComparison.Ordinal)) return result;
            foreach (string line in text.Split('\n'))
            {
                int separator = line.IndexOf('|');
                if (separator != 64) continue;
                string hash = line.Substring(0,64), name = line.Substring(65).TrimEnd('\r');
                bool valid = true;
                foreach (char value in hash) if (!Uri.IsHexDigit(value)) valid = false;
                if (valid && name.Length < 200) result[name] = hash.ToLowerInvariant();
            }
            return result;
        }
        internal static string ReadInstalledManifest(string directory)
        {
            string path = Path.Combine(directory, ManifestName);
            if (!File.Exists(path) || new FileInfo(path).Length > 128 * 1024 || (File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0) return null;
            return File.ReadAllText(path);
        }
        internal static int PruneObsolete(string directory, string previous, string current)
        {
            Dictionary<string,string> keep = ParseManifest(current);
            if (keep.Count == 0) return 0;
            int removed = 0;
            foreach (KeyValuePair<string,string> entry in ParseManifest(previous))
            {
                if (keep.ContainsKey(entry.Key)) continue;
                string path = OwnedPath(directory,entry.Key);
                if (path == null || !File.Exists(path)) continue;
                using (FileStream file = File.OpenRead(path)) if (Hash(file) != entry.Value) continue;
                try { File.Delete(path); removed++; } catch (IOException) { } catch (UnauthorizedAccessException) { }
            }
            return removed;
        }
        internal static void FinalizeInstalled(string directory, string previous)
        {
            VerifyInstalled(directory);
            string manifest;
            using (Stream source = Assembly.GetExecutingAssembly().GetManifestResourceStream("SnowDown.Manifest"))
            using (StreamReader reader = new StreamReader(source, Encoding.UTF8)) manifest = reader.ReadToEnd();
            Dictionary<string,string> expected = ParseManifest(manifest);
            if (expected.Count < 5) throw new IOException("程序文件清单无效，请重新下载安装包。");
            foreach (KeyValuePair<string,string> entry in expected)
            {
                string path = OwnedPath(directory,entry.Key);
                if (path == null || !File.Exists(path)) throw new IOException("安装文件不完整，请重试。");
                using (FileStream file = File.OpenRead(path)) if (Hash(file) != entry.Value) throw new IOException("安装文件校验未通过，请重试。");
            }
            PruneObsolete(directory,previous,manifest);
            string destination = Path.Combine(directory,ManifestName);
            string temporary = destination + ".tmp";
            File.WriteAllText(temporary,manifest,Encoding.UTF8);
            if (File.Exists(destination)) File.Replace(temporary,destination,null); else File.Move(temporary,destination);
        }
        internal static string UpdateDirectory(string[] args)
        {
            foreach (string argument in args)
                if (argument.StartsWith("--update-dir=", StringComparison.Ordinal)) return NormalizePath(argument.Substring("--update-dir=".Length));
            return null;
        }

        internal static void WaitForUpdatingClient(string[] args)
        {
            foreach (string argument in args)
            {
                if (!argument.StartsWith("--wait-pid=", StringComparison.Ordinal)) continue;
                int id;
                if (!Int32.TryParse(argument.Substring("--wait-pid=".Length), out id) || id <= 0 || id == Process.GetCurrentProcess().Id)
                    throw new ArgumentException("无法识别待退出的软件进程。");
                try
                {
                    using (Process previous = Process.GetProcessById(id))
                        if (!previous.WaitForExit(30000)) throw new InvalidOperationException("SnowDown 尚未退出，请从系统托盘退出后重试更新。");
                }
                catch (ArgumentException) { /* The parent already exited; do not stop another process. */ }
            }
        }

        internal static string NormalizePath(string value)
        {
            if (String.IsNullOrWhiteSpace(value) || value.IndexOfAny(new[] { '"', '\r', '\n', '\0', '<', '>', '|', '*', '?' }) >= 0)
                throw new ArgumentException("请填写有效的安装位置。");
            value = value.Trim();
            if (!Path.IsPathRooted(value) || value.StartsWith(@"\\", StringComparison.Ordinal) || value.Length < 3 || value[1] != ':' || value[2] != '\\')
                throw new ArgumentException("请选择本机磁盘上的完整路径，例如 D:\\Apps\\SnowDown。");
            string full = Path.GetFullPath(value).TrimEnd('\\');
            if (full.Length < 4 || full.Length > 180)
                throw new ArgumentException("请选择独立的软件文件夹，路径长度不要超过 180 个字符。");
            return full;
        }

        internal static string DefaultDirectory()
        {
            // Tauri's current-user NSIS installation uses the 64-bit registry view.
            using (RegistryKey user = RegistryKey.OpenBaseKey(RegistryHive.CurrentUser, RegistryView.Registry64))
            using (RegistryKey key = user.OpenSubKey(@"Software\snowdown\SnowDown"))
            {
                string previous = key == null ? null : key.GetValue(null) as string;
                if (!String.IsNullOrWhiteSpace(previous))
                {
                    try { return NormalizePath(previous); } catch (ArgumentException) { }
                }
            }
            return Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "SnowDown");
        }

        internal static void CheckDirectory(string directory)
        {
            string path = NormalizePath(directory);
            DriveInfo drive = new DriveInfo(Path.GetPathRoot(path));
            if (!drive.IsReady || drive.AvailableFreeSpace < 250L * 1024 * 1024)
                throw new IOException("安装需要至少 250 MB 可用空间，请选择其他位置。");
            foreach (Process process in Process.GetProcessesByName("snowdown"))
            {
                using (process)
                {
                    try
                    {
                        if (String.Equals(process.MainModule.FileName, Path.Combine(path, "snowdown.exe"), StringComparison.OrdinalIgnoreCase))
                            throw new InvalidOperationException("请先从系统托盘退出 SnowDown，再继续安装。");
                    }
                    catch (System.ComponentModel.Win32Exception) { }
                }
            }
            Directory.CreateDirectory(path);
            string probe = Path.Combine(path, ".snowdown-write-" + Guid.NewGuid().ToString("N"));
            using (FileStream file = new FileStream(probe, FileMode.CreateNew, FileAccess.Write, FileShare.None, 1, FileOptions.DeleteOnClose))
                file.WriteByte(0);
        }

        internal static string Hash(Stream stream)
        {
            using (SHA256 sha = SHA256.Create())
                return BitConverter.ToString(sha.ComputeHash(stream)).Replace("-", "").ToLowerInvariant();
        }

        internal static string ExtractPayload(string directory)
        {
            Directory.CreateDirectory(directory);
            string destination = Path.Combine(directory, "payload.exe");
            using (Stream source = Assembly.GetExecutingAssembly().GetManifestResourceStream("SnowDown.Payload"))
            {
                if (source == null) throw new IOException("安装文件缺失，请重新下载完整安装包。");
                using (FileStream file = new FileStream(destination, FileMode.CreateNew, FileAccess.Write, FileShare.None))
                    source.CopyTo(file);
            }
            using (FileStream file = File.OpenRead(destination))
                if (Hash(file) != BuildInfo.PayloadHash) throw new IOException("安装文件校验失败，请重新下载安装包。");
            return destination;
        }

        internal static string BackendArguments(string directory, bool update = false)
        {
            // NSIS requires /D last, without quotation marks, even when the path has spaces.
            return (update ? "/S /UPDATE /NS /D=" : "/S /D=") + NormalizePath(directory);
        }

        internal static Process StartBackend(string payload, string directory, string statusFile, bool update = false)
        {
            CheckOverwrite(directory);
            // Existing installations always use NSIS's in-place upgrade branch,
            // including manual/silent installers and versions predating the manifest.
            bool overwrite = update || File.Exists(Path.Combine(directory, "snowdown.exe")) || File.Exists(Path.Combine(directory, "uninstall.exe"));
            Log("Starting backend; in-place=" + overwrite + "; target=" + NormalizePath(directory));
            ProcessStartInfo start = new ProcessStartInfo(payload, BackendArguments(directory,overwrite));
            start.UseShellExecute = false;
            start.CreateNoWindow = true;
            start.WorkingDirectory = Path.GetDirectoryName(payload);
            start.EnvironmentVariables["SNOWDOWN_SETUP_STATUS"] = statusFile;
            return Process.Start(start);
        }

        internal static void VerifyInstalled(string directory)
        {
            if (!File.Exists(Path.Combine(directory, "snowdown.exe")) || !File.Exists(Path.Combine(directory, "uninstall.exe")) || !File.Exists(Path.Combine(directory, "resources", "ffmpeg.exe")))
                throw new IOException("安装文件不完整，请检查磁盘空间后重试。");
        }

        internal static string StageText(string stage)
        {
            switch (stage)
            {
                case "webview": return "正在检查并准备 WebView2 运行组件…";
                case "files": return "正在安装 SnowDown 和音视频组件…";
                case "registering": return "正在创建快捷方式并完成安装…";
                case "complete": return "安装完成，即刻开始收藏。";
                case "running": return "请先从系统托盘退出 SnowDown，再重试安装。";
                case "failed": return "安装未完成，请检查磁盘空间和文件权限后重试。";
                default: return "正在准备安装文件…";
            }
        }

        internal static void Cleanup(string directory)
        {
            // Only the unique temporary folder created by this installer is eligible.
            string expectedParent = Path.Combine(Path.GetTempPath(), "SnowDown-Setup");
            string parent = Path.GetDirectoryName(Path.GetFullPath(directory));
            if (!String.Equals(parent, expectedParent.TrimEnd('\\'), StringComparison.OrdinalIgnoreCase)) return;
            try { Directory.Delete(directory, true); } catch (IOException) { } catch (UnauthorizedAccessException) { }
        }
    }
}

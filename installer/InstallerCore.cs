using System;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Security.Cryptography;
using Microsoft.Win32;

namespace SnowDown.Setup
{
    internal static class InstallerCore
    {
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

        internal static string BackendArguments(string directory)
        {
            // NSIS requires /D last, without quotation marks, even when the path has spaces.
            return "/S /D=" + NormalizePath(directory);
        }

        internal static Process StartBackend(string payload, string directory, string statusFile)
        {
            ProcessStartInfo start = new ProcessStartInfo(payload, BackendArguments(directory));
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

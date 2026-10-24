using System;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Threading;
using System.Threading.Tasks;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Controls.Primitives;
using System.Windows.Input;
using System.Windows.Markup;
using System.Windows.Media;
using System.Windows.Media.Imaging;
using System.Windows.Threading;
using Forms = System.Windows.Forms;

namespace SnowDown.Setup
{
    internal static class Program
    {
        [STAThread]
        private static int Main(string[] args)
        {
            try
            {
                if (args.Length == 1 && args[0] == "--verify-payload")
                {
                    string temporary = NewTemporaryDirectory();
                    try { InstallerCore.ExtractPayload(temporary); return 0; }
                    finally { InstallerCore.Cleanup(temporary); }
                }
                if (args.Length > 0 && args[0].StartsWith("--render-preview=", StringComparison.Ordinal))
                {
                    // Off-screen rendering of our own layout for design QA; no desktop input.
                    SetupWindow preview = new SetupWindow(true);
                    preview.ExportPreview(args[0].Substring("--render-preview=".Length), args.Length > 1 ? args[1] : "ready");
                    return 0;
                }
                foreach (string argument in args)
                    if (argument == "/S") return SilentInstall(args);
                using (Mutex mutex = new Mutex(false, @"Local\SnowDown-Setup-CurrentUser"))
                {
                    bool owns = false;
                    try
                    {
                        try { owns = mutex.WaitOne(0); } catch (AbandonedMutexException) { owns = true; }
                        if (!owns) return 0;
                        Application application = new Application();
                        SetupWindow setup = new SetupWindow(false);
                        return application.Run(setup.Window);
                    }
                    finally { if (owns) mutex.ReleaseMutex(); }
                }
            }
            catch (Exception error)
            {
                // No command line or credentials are written to logs.
                if (args.Length == 0) MessageBox.Show("安装器无法启动：" + error.Message, "SnowDown", MessageBoxButton.OK, MessageBoxImage.Information);
                return 1;
            }
        }

        private static int SilentInstall(string[] args)
        {
            string directory = InstallerCore.DefaultDirectory();
            foreach (string argument in args)
                if (argument.StartsWith("/D=", StringComparison.Ordinal)) directory = argument.Substring(3);
            InstallerCore.CheckDirectory(directory);
            string temporary = NewTemporaryDirectory();
            try
            {
                string payload = InstallerCore.ExtractPayload(temporary);
                using (Process backend = InstallerCore.StartBackend(payload, directory, Path.Combine(temporary, "stage")))
                {
                    backend.WaitForExit();
                    if (backend.ExitCode != 0 && backend.ExitCode != 3010) return backend.ExitCode;
                    InstallerCore.VerifyInstalled(directory);
                    return backend.ExitCode;
                }
            }
            finally { InstallerCore.Cleanup(temporary); }
        }

        internal static string NewTemporaryDirectory()
        {
            return Path.Combine(Path.GetTempPath(), "SnowDown-Setup", Guid.NewGuid().ToString("N"));
        }
    }

    internal sealed class SetupWindow
    {
        internal readonly Window Window;
        private readonly bool preview;
        private readonly Canvas design;
        private readonly TextBox installPath;
        private readonly Button action;
        private readonly Button browse;
        private readonly Button close;
        private readonly TextBlock actionText;
        private readonly TextBlock status;
        private readonly TextBlock progressTitle;
        private readonly TextBlock progressDetail;
        private readonly CheckBox launch;
        private readonly Grid readyPanel;
        private readonly StackPanel progressPanel;
        private readonly ProgressBar progress;
        private bool busy;
        private bool complete;
        private string installedDirectory;

        internal SetupWindow(bool preview)
        {
            this.preview = preview;
            using (Stream source = Assembly.GetExecutingAssembly().GetManifestResourceStream("SnowDown.Layout"))
                Window = (Window)XamlReader.Load(source);
            using (Stream source = Assembly.GetExecutingAssembly().GetManifestResourceStream("SnowDown.Character"))
            {
                BitmapImage bitmap = new BitmapImage();
                bitmap.BeginInit(); bitmap.CacheOption = BitmapCacheOption.OnLoad; bitmap.StreamSource = source; bitmap.EndInit(); bitmap.Freeze();
                ((Image)Window.FindName("Character")).Source = bitmap;
            }
            design = (Canvas)Window.FindName("Design");
            installPath = (TextBox)Window.FindName("InstallPath");
            action = (Button)Window.FindName("Install");
            browse = (Button)Window.FindName("Browse");
            close = (Button)Window.FindName("Close");
            actionText = (TextBlock)Window.FindName("ActionText");
            status = (TextBlock)Window.FindName("Status");
            progressTitle = (TextBlock)Window.FindName("ProgressTitle");
            progressDetail = (TextBlock)Window.FindName("ProgressDetail");
            readyPanel = (Grid)Window.FindName("ReadyPanel");
            progressPanel = (StackPanel)Window.FindName("ProgressPanel");
            progress = (ProgressBar)Window.FindName("Progress");
            launch = (CheckBox)Window.FindName("LaunchAfter");
            installPath.Text = preview ? @"D:\Apps\SnowDown" : InstallerCore.DefaultDirectory();
            ((TextBlock)Window.FindName("Version")).Text = "v" + BuildInfo.Version + " · Windows x64";
            using (Stream icon = Assembly.GetExecutingAssembly().GetManifestResourceStream("SnowDown.Icon"))
                Window.Icon = BitmapFrame.Create(icon, BitmapCreateOptions.None, BitmapCacheOption.OnLoad);
            ConnectEvents();
        }

        private void ConnectEvents()
        {
            ((Button)Window.FindName("Minimize")).Click += delegate { Window.WindowState = WindowState.Minimized; };
            close.Click += delegate { if (!busy) Window.Close(); };
            Window.Closing += delegate(object sender, CancelEventArgs e) { if (busy) { e.Cancel = true; status.Text = "正在安装，请等待完成后关闭。"; } };
            design.MouseLeftButtonDown += delegate(object sender, MouseButtonEventArgs e)
            {
                DependencyObject source = e.OriginalSource as DependencyObject;
                while (source != null)
                {
                    if (source is ButtonBase || source is TextBoxBase) return;
                    source = source is Visual ? VisualTreeHelper.GetParent(source) : LogicalTreeHelper.GetParent(source);
                }
                if (e.ButtonState == MouseButtonState.Pressed) Window.DragMove();
            };
            browse.Click += delegate
            {
                using (Forms.FolderBrowserDialog picker = new Forms.FolderBrowserDialog())
                {
                    picker.Description = "选择 SnowDown 安装文件夹";
                    picker.SelectedPath = installPath.Text;
                    if (picker.ShowDialog(new WindowOwner(Window)) == Forms.DialogResult.OK)
                    {
                        string directory = picker.SelectedPath;
                        installPath.Text = String.Equals(Path.GetFileName(directory), "SnowDown", StringComparison.OrdinalIgnoreCase) ? directory : Path.Combine(directory, "SnowDown");
                    }
                }
            };
            action.Click += async delegate
            {
                if (preview) return;
                if (complete) { Finish(); return; }
                await Install();
            };
            Window.Loaded += delegate
            {
                double availableWidth = SystemParameters.WorkArea.Width - 32;
                double availableHeight = SystemParameters.WorkArea.Height - 32;
                double scale = Math.Min(1.0, Math.Min(availableWidth / 1000, availableHeight / 660));
                Window.Width = 1000 * scale; Window.Height = 660 * scale;
                Window.Left = SystemParameters.WorkArea.Left + (SystemParameters.WorkArea.Width - Window.Width) / 2;
                Window.Top = SystemParameters.WorkArea.Top + (SystemParameters.WorkArea.Height - Window.Height) / 2;
            };
        }

        private async Task Install()
        {
            if (busy) return;
            string temporary = Program.NewTemporaryDirectory();
            DispatcherTimer timer = null;
            try
            {
                installedDirectory = InstallerCore.NormalizePath(installPath.Text);
                InstallerCore.CheckDirectory(installedDirectory);
                SetBusy();
                string statusFile = Path.Combine(temporary, "stage");
                timer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(250) };
                timer.Tick += delegate
                {
                    try { if (File.Exists(statusFile)) progressDetail.Text = InstallerCore.StageText(File.ReadAllText(statusFile).Trim()); }
                    catch (IOException) { }
                };
                timer.Start();
                int exitCode = await Task.Run(delegate
                {
                    string payload = InstallerCore.ExtractPayload(temporary);
                    using (Process backend = InstallerCore.StartBackend(payload, installedDirectory, statusFile))
                    {
                        backend.WaitForExit();
                        return backend.ExitCode;
                    }
                });
                timer.Stop();
                if (exitCode != 0 && exitCode != 3010)
                {
                    string stage = File.Exists(statusFile) ? File.ReadAllText(statusFile).Trim() : "failed";
                    throw new IOException(stage == "running" ? InstallerCore.StageText(stage) : "安装未完成（" + exitCode + "），请检查磁盘空间和文件权限后重试。");
                }
                InstallerCore.VerifyInstalled(installedDirectory);
                SetComplete();
                if (exitCode == 3010) status.Text = "组件更新将在重新启动 Windows 后完成。";
            }
            catch (Exception error)
            {
                busy = false; complete = false;
                readyPanel.Visibility = Visibility.Visible;
                progressPanel.Visibility = Visibility.Collapsed;
                action.IsEnabled = true; browse.IsEnabled = true; close.IsEnabled = true;
                installPath.IsEnabled = true; launch.IsEnabled = true;
                actionText.Text = "重试安装";
                status.Foreground = new SolidColorBrush(Color.FromRgb(194, 102, 111));
                status.Text = error.Message;
            }
            finally
            {
                if (timer != null) timer.Stop();
                InstallerCore.Cleanup(temporary);
            }
        }

        private void SetBusy()
        {
            busy = true; status.Text = "安装期间可最小化窗口。";
            status.Foreground = new SolidColorBrush(Color.FromRgb(143, 164, 191));
            readyPanel.Visibility = Visibility.Collapsed;
            progressPanel.Visibility = Visibility.Visible;
            progressTitle.Text = "正在安装";
            progressDetail.Text = "正在准备安装文件…";
            progress.IsIndeterminate = true;
            actionText.Text = "正在安装…";
            action.IsEnabled = false; browse.IsEnabled = false; close.IsEnabled = false;
            installPath.IsEnabled = false; launch.IsEnabled = false;
        }

        private void SetComplete()
        {
            busy = false; complete = true;
            readyPanel.Visibility = Visibility.Collapsed;
            progressPanel.Visibility = Visibility.Visible;
            progressTitle.Text = "安装完成";
            progressDetail.Text = "SnowDown 已准备就绪，即刻开始收藏。";
            progress.IsIndeterminate = false; progress.Value = 100;
            actionText.Text = launch.IsChecked == true ? "完成并启动" : "完成安装";
            action.IsEnabled = true; close.IsEnabled = true; launch.IsEnabled = false;
            ((System.Windows.Shapes.Path)Window.FindName("DownloadIcon")).Data = Geometry.Parse("M 0,12 L 8,20 L 24,0");
            status.Text = "文件已安装到你选择的位置。";
        }

        private void Finish()
        {
            if (launch.IsChecked == true)
            {
                try
                {
                    string executable = Path.Combine(installedDirectory, "snowdown.exe");
                    // A portable copy shares the client's single-instance identity. Do not
                    // let it make a successful install appear to reopen the old version.
                    foreach (Process process in Process.GetProcessesByName("snowdown"))
                    {
                        using (process)
                        {
                            try
                            {
                                if (!String.Equals(process.MainModule.FileName, executable, StringComparison.OrdinalIgnoreCase))
                                {
                                    status.Text = "请先从托盘退出已打开的 SnowDown，再启动新版本。";
                                    return;
                                }
                            }
                            catch (Win32Exception) { }
                            catch (InvalidOperationException) { }
                        }
                    }
                    Process.Start(new ProcessStartInfo(executable) { UseShellExecute = true, WorkingDirectory = installedDirectory });
                }
                catch (Exception) { status.Text = "软件已安装。可从桌面或开始菜单打开 SnowDown。"; return; }
            }
            Window.Close();
        }

        internal void ExportPreview(string destination, string state)
        {
            if (state == "progress") SetBusy();
            else if (state == "complete") SetComplete();
            design.Measure(new Size(1000, 660));
            design.Arrange(new Rect(0, 0, 1000, 660));
            design.UpdateLayout();
            RenderTargetBitmap bitmap = new RenderTargetBitmap(1500, 990, 144, 144, PixelFormats.Pbgra32);
            bitmap.Render(design);
            PngBitmapEncoder encoder = new PngBitmapEncoder();
            encoder.Frames.Add(BitmapFrame.Create(bitmap));
            using (FileStream file = File.Create(destination)) encoder.Save(file);
        }

        private sealed class WindowOwner : Forms.IWin32Window
        {
            private readonly Window window;
            internal WindowOwner(Window window) { this.window = window; }
            public IntPtr Handle { get { return new System.Windows.Interop.WindowInteropHelper(window).Handle; } }
        }
    }
}

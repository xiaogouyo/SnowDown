using System;
using System.IO;
using System.Text;

namespace SnowDown.Setup
{
    internal static class InstallerCoreTests
    {
        private static int assertions;
        private static void Check(bool valid, string label)
        {
            if (!valid) throw new Exception("FAILED: " + label);
            assertions++;
        }
        private static void Reject(string path)
        {
            try { InstallerCore.NormalizePath(path); throw new Exception("Accepted invalid path: " + path); }
            catch (ArgumentException) { assertions++; }
        }
        private static int Main(string[] args)
        {
            if (args.Length == 1 && args[0] == "--wait-fixture") { System.Threading.Thread.Sleep(300); return 0; }
            Check(InstallerCore.DefaultAutostart(false,false,false),"fresh install defaults to startup");
            Check(!InstallerCore.DefaultAutostart(true,false,false),"upgrade preserves opt-out");
            Check(InstallerCore.DefaultAutostart(true,true,false),"upgrade preserves opt-in");
            Check(!InstallerCore.DefaultAutostart(true,true,true),"upgrade respects Windows disable");
            Check(InstallerCore.OwnedStartup("\"D:\\应用 文件\\SnowDown.exe\" --autostart"),"quoted startup command recognized");
            Check(!InstallerCore.OwnedStartup("\" --autostart"),"malformed startup command rejected");
            Check(!InstallerCore.OwnedStartup("\"D:\\other.exe\" --autostart"),"unrelated startup command rejected");
            Check(InstallerCore.NormalizePath(@"D:\应用 文件\SnowDown\") == @"D:\应用 文件\SnowDown", "Unicode path");
            Check(InstallerCore.BackendArguments(@"D:\应用 文件\SnowDown") == @"/S /D=D:\应用 文件\SnowDown", "NSIS last unquoted directory");
            Check(InstallerCore.UpdateDirectory(new[] { @"--update-dir=D:\应用 文件\SnowDown", "--wait-pid=123" }) == @"D:\应用 文件\SnowDown", "updater keeps current Unicode directory");
            Check(InstallerCore.UpdateDirectory(new string[0]) == null, "normal installer uses default directory");
            try { InstallerCore.UpdateDirectory(new[] { "--update-dir=relative" }); throw new Exception("Accepted relative update directory"); } catch (ArgumentException) { assertions++; }
            try { InstallerCore.WaitForUpdatingClient(new[] { "--wait-pid=invalid" }); throw new Exception("Accepted invalid parent id"); } catch (ArgumentException) { assertions++; }
            using (System.Diagnostics.Process fixture = System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(System.Reflection.Assembly.GetExecutingAssembly().Location, "--wait-fixture") { UseShellExecute = false, CreateNoWindow = true }))
            {
                InstallerCore.WaitForUpdatingClient(new[] { "--wait-pid=" + fixture.Id });
                Check(fixture.HasExited, "updater waits for parent to exit without terminating it");
            }
            Reject(@"D:\"); Reject(@"D:relative"); Reject("relative"); Reject(@"\\server\share\SnowDown");
            Reject("D:\\Apps\\SnowDown\" /R"); Reject("D:\\Apps\\SnowDown\n/R");
            using (MemoryStream bytes = new MemoryStream(Encoding.UTF8.GetBytes("abc")))
                Check(InstallerCore.Hash(bytes) == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "SHA256 payload integrity");
            Check(InstallerCore.StageText("running").Contains("退出"), "busy app message");
            Check(InstallerCore.StageText("files").Contains("音视频"), "real backend phase");
            string temporary = Path.Combine(Path.GetTempPath(), "SnowDown-CoreTest-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(temporary);
            InstallerCore.Cleanup(temporary);
            Check(Directory.Exists(temporary), "cleanup refuses a directory it did not create");
            Directory.Delete(temporary);
            Check(InstallerCore.BackendArguments(@"D:\SnowDown",true)==@"/S /UPDATE /NS /D=D:\SnowDown","automatic update preserves data and avoids shortcuts");
            Check(InstallerCore.AutomaticUpdate(new[]{"--auto-update",@"--update-dir=D:\SnowDown","--wait-pid=123"}),"automatic update requires complete arguments");
            string cleanup=Path.Combine(Path.GetTempPath(),"SnowDown-Prune-"+Guid.NewGuid().ToString("N"));Directory.CreateDirectory(Path.Combine(cleanup,"resources"));
            string digest;using(MemoryStream data=new MemoryStream(Encoding.UTF8.GetBytes("old")))digest=InstallerCore.Hash(data);
            foreach(string file in new[]{"resources/old.dll","resources/modified.dll","Cookies","settings.json","video.mp4"})File.WriteAllText(Path.Combine(cleanup,file),file.Contains("modified")?"user-changed":"old",new UTF8Encoding(false));
            string old="SnowDown-owned-files-v1\n";foreach(string file in new[]{"resources/old.dll","resources/modified.dll","Cookies","settings.json","video.mp4","../escape.dll"})old+=digest+"|"+file+"\n";
            Check(InstallerCore.PruneObsolete(cleanup,old,"SnowDown-owned-files-v1\n"+digest+"|snowdown.exe\n")==1,"remove only unchanged obsolete program files");
            foreach(string file in new[]{"resources/modified.dll","Cookies","settings.json","video.mp4"})Check(File.Exists(Path.Combine(cleanup,file)),"preserve user data or modified file: "+file);
            Directory.Delete(cleanup,true);
            string verification = Path.Combine(Path.GetTempPath(), "SnowDown-Verify-" + Guid.NewGuid().ToString("N"));
            Directory.CreateDirectory(Path.Combine(verification, "resources"));
            string verificationManifest = "SnowDown-owned-files-v1\n";
            string[] verifiedFiles = { "snowdown.exe", "resources/ffmpeg.exe", "resources/NOTICE.txt", "resources/FFMPEG-LICENSE.txt", "resources/ffmpeg-build.json" };
            string previousLog = InstallerCore.LogPath;
            InstallerCore.LogPath = Path.Combine(verification, "setup.log");
            try
            {
                foreach (string file in verifiedFiles)
                {
                    File.WriteAllText(Path.Combine(verification, file), "old", new UTF8Encoding(false));
                    verificationManifest += digest + "|" + file + "\n";
                }
                InstallerCore.VerifyManifest(verification, verificationManifest);
                Check(true, "matching payload files pass verification");
                File.WriteAllText(Path.Combine(verification, "snowdown.exe"), "changed", new UTF8Encoding(false));
                try { InstallerCore.VerifyManifest(verification, verificationManifest); throw new Exception("Accepted changed executable"); }
                catch (IOException error) { Check(error.Message.Contains("snowdown.exe"), "hash failure identifies the exact file"); }
                string diagnostic = File.ReadAllText(InstallerCore.LogPath);
                Check(diagnostic.Contains("hash mismatch: snowdown.exe; expected=" + digest + "; actual="), "hash failure logs expected and actual digests");
                File.WriteAllText(Path.Combine(verification, "snowdown.exe"), "old", new UTF8Encoding(false));
                File.Delete(Path.Combine(verification, "resources", "NOTICE.txt"));
                try { InstallerCore.VerifyManifest(verification, verificationManifest); throw new Exception("Accepted missing resource"); }
                catch (IOException error) { Check(error.Message.Contains("resources/NOTICE.txt"), "missing file identifies the exact resource"); }
            }
            finally { InstallerCore.LogPath = previousLog; Directory.Delete(verification, true); }
            Console.WriteLine("Installer core: " + assertions + " assertions passed.");
            return 0;
        }
    }
}

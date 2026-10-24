# 更新下载镜像库

此库保留用户提供的全部 66 个地址和原始编号。每次下载重新检测，不采用历史表格中的延迟、速度或状态。

版本信息、安装器文件名、大小和 SHA256 从 SnowDown 官方 GitHub Release 获取。节点按 https://镜像域名/https://github.com/... 的格式代理当前安装器。并发检测 12 个节点，每个最多 5 秒，请求文件前 4 KiB，核对响应状态、大小或 Content-Range 以及 Windows PE 文件开头；延迟从发起请求到收到有效文件开头计算，包含连接与服务器响应时间。这里的延迟不是 ICMP ping，也不代表持续下载速度。

检测完成后，按本次延迟升序选择可用节点。失败时使用下一个候选，最后尝试 GitHub 官方直连；每次切换重新建立临时文件，整包大小与官方 SHA256 必须匹配才保存。已完整校验的本地缓存可以直接复用。测速和传输均可取消，镜像表中可查看状态和失败原因。

只允许 HTTPS 和镜像库或 GitHub 下载主机间的有限跳转，不向镜像发送 GitHub 令牌或视频平台登录信息。检测会访问这些第三方节点；节点可能随时失效，不保证全部可用。

| ID | 镜像 |
| --- | --- |
| 1 | down.mxw.qzz.io |
| 2 | tvv.tw |
| 3 | xsadwsd.kdns.fr |
| 4 | fastgit.cc |
| 5 | gh.monlor.com |
| 6 | ghproxy.imciel.com |
| 7 | gh.acmsz.top |
| 8 | gh.07150721.xyz |
| 9 | cdn.akaere.online |
| 10 | git.yylx.win |
| 11 | down.mxw.xx.kg |
| 12 | ghfast.top |
| 13 | gh.my-website.ccwu.cc |
| 14 | github.mxw.qzz.io |
| 15 | gh-proxy.com |
| 16 | ghproxy.felicity.land |
| 17 | github.tbap.top |
| 18 | gh.noki.icu |
| 19 | cdn.gh-proxy.com |
| 20 | gh.sixyin.com |
| 21 | free.cn.eu.org |
| 22 | git.669966.xyz |
| 23 | gh.b52m.cn |
| 24 | github.chenc.dev |
| 25 | gh.dpik.top |
| 26 | ghproxy.net |
| 27 | g.blfrp.cn |
| 28 | github.dpik.top |
| 29 | ghfile.geekertao.top |
| 30 | gh.bugdey.us.kg |
| 31 | github-proxy.memory-echoes.cn |
| 32 | gh.927223.xyz |
| 33 | gh.inkchills.cn |
| 34 | jiashu.1win.eu.org |
| 35 | gh.felicity.ac.cn |
| 36 | gitproxy.mrhjx.cn |
| 37 | gh.ddlc.top |
| 38 | ghf.xn--eqrr82bzpe.top |
| 39 | gh.catmak.name |
| 40 | gitproxy.127731.xyz |
| 41 | ghpxy.hwinzniej.top |
| 42 | ghm.078465.xyz |
| 43 | gh.jasonzeng.dev |
| 44 | gp.zkitefly.eu.org |
| 45 | github.geekery.cn |
| 46 | github.ednovas.xyz |
| 47 | ghproxy.1888866.xyz |
| 48 | ghproxy.cxkpro.top |
| 49 | gh.idayer.com |
| 50 | gh.chjina.com |
| 51 | ghp.keleyaa.com |
| 52 | ghp.arslantu.xyz |
| 53 | githubdog.com |
| 54 | gh.meali.top |
| 55 | 777.z321.cc.cd |
| 56 | g.z321.cc.cd |
| 57 | gg.z321.cc.cd |
| 58 | js.jiangss.shop |
| 59 | github.ikgy.top |
| 60 | cfgh.ikgy.top |
| 61 | ghproxy.monkeyray.net |
| 62 | gh.qfmc0721.cc.cd |
| 63 | github.xxlab.tech |
| 64 | proxy.yaoyaoling.net |
| 65 | gh.ruan.dpdns.org |
| 66 | github.nswrz.cn |

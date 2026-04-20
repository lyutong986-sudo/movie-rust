.PHONY: diagnose

diagnose:
	@echo "开始收集诊断信息..."
	@mkdir -p diagnose-temp
	@echo "1. 收集容器日志..."
	@docker-compose logs --no-color > diagnose-temp/docker-compose.log 2>&1 || true
	@echo "2. 收集网络抓包（需要 tcpdump 权限）..."
	@echo "   （跳过，需要手动执行：docker run --network movie-rust_default -v $(pwd)/logs:/capture tcpdump -i any -w /capture/network-capture.pcap）"
	@echo "3. 导出数据库..."
	@docker-compose exec -T postgres pg_dump -U movie movie_rust > diagnose-temp/database-dump.sql 2>/dev/null || echo "无法导出数据库"
	@echo "4. 收集客户端错误日志..."
	@if [ -f logs/client-error.log ]; then cp logs/client-error.log diagnose-temp/; fi
	@echo "5. 打包诊断文件..."
	@tar czf diagnose-$(shell date +%Y%m%d-%H%M%S).tar.gz diagnose-temp/
	@rm -rf diagnose-temp
	@echo "诊断包已生成：diagnose-*.tar.gz"
FROM swr.cn-north-4.myhuaweicloud.com/ddn-k8s/docker.io/shinyay/docker-mvn-jdk8:3.6.3 as builder

ARG VERSION

COPY ./target/apache-seatunnel-${VERSION}-bin.tar.gz /opt/
RUN cd /opt && \
    tar -zxvf apache-seatunnel-${VERSION}-bin.tar.gz && \
    mv apache-seatunnel-${VERSION} seatunnel && \
#    cp seatunnel/starter/seatunnel-starter-${VERSION}.jar seatunnel/starter/seatunnel-starter.jar && \
#    cp seatunnel/starter/seatunnel-flink-13-starter-${VERSION}.jar seatunnel/starter/seatunnel-flink-13-starter.jar && \
#    cp seatunnel/starter/seatunnel-flink-15-starter-${VERSION}.jar seatunnel/starter/seatunnel-flink-15-starter.jar && \
#    cp seatunnel/starter/seatunnel-spark-2-starter-${VERSION}.jar seatunnel/starter/seatunnel-spark-2-starter.jar && \
#    cp seatunnel/starter/seatunnel-spark-3-starter-${VERSION}.jar seatunnel/starter/seatunnel-spark-3-starter.jar && \
    rm apache-seatunnel-${VERSION}-bin.tar.gz && \
    sed -i 's/#rootLogger.appenderRef.consoleStdout.ref/rootLogger.appenderRef.consoleStdout.ref/' seatunnel/config/log4j2.properties && \
    sed -i 's/#rootLogger.appenderRef.consoleStderr.ref/rootLogger.appenderRef.consoleStderr.ref/' seatunnel/config/log4j2.properties && \
    sed -i 's/rootLogger.appenderRef.file.ref/#rootLogger.appenderRef.file.ref/' seatunnel/config/log4j2.properties && \
    cp seatunnel/config/hazelcast-master.yaml seatunnel/config/hazelcast-worker.yaml

FROM swr.cn-north-4.myhuaweicloud.com/ddn-k8s/docker.io/shinyay/docker-mvn-jdk8:3.6.3
COPY --from=builder /opt/seatunnel /opt/seatunnel
WORKDIR /opt/seatunnel